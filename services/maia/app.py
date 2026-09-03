import json
import math
import os
import sys
import time
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from importlib.metadata import version
from threading import BoundedSemaphore
from typing import Mapping

from model_artifacts import finalize_model_files, prepare_model_files

MAX_CONCURRENT_PREDICTIONS = 4
TORCH_INTRAOP_THREADS = 2
TORCH_INTEROP_THREADS = 1


def emit_service_log(level: str, event: str, **fields: object) -> None:
    stream = sys.stderr if level == "error" else sys.stdout
    print(
        json.dumps(
            {"level": level, "event": event, **fields},
            separators=(",", ":"),
        ),
        file=stream,
        flush=True,
    )


@dataclass(frozen=True)
class PredictionRequest:
    position: str
    player_elo: int
    opponent_elo: int
    limit: int


@dataclass(frozen=True)
class RuntimeIdentity:
    package: str
    model: str


def parse_request(payload: object) -> PredictionRequest:
    if not isinstance(payload, dict):
        raise ValueError("request body must be a JSON object")

    position = payload.get("position")
    if not isinstance(position, str) or not position.strip():
        raise ValueError("position must be a non-empty FEN string")

    player_elo = bounded_integer(payload.get("playerElo"), "playerElo", 100, 3500)
    opponent_elo = bounded_integer(payload.get("opponentElo"), "opponentElo", 100, 3500)
    limit = bounded_integer(payload.get("limit"), "limit", 1, 20)
    return PredictionRequest(position, player_elo, opponent_elo, limit)


def bounded_integer(value: object, name: str, minimum: int, maximum: int) -> int:
    if type(value) is not int or not minimum <= value <= maximum:
        raise ValueError(
            f"{name} must be a whole number between {minimum} and {maximum}"
        )
    return value


def format_prediction(
    move_probabilities: Mapping[str, object], win_probability: object, limit: int
) -> dict[str, object]:
    if not move_probabilities:
        raise ValueError("Maia-2 returned no move candidates")

    candidates = []
    for uci, raw_probability in move_probabilities.items():
        probability = float(raw_probability)
        if not uci or not math.isfinite(probability) or not 0.0 <= probability <= 1.0:
            raise ValueError("Maia-2 returned an invalid move probability")
        candidates.append({"uci": uci, "probability": probability})
    candidates.sort(key=lambda candidate: candidate["probability"], reverse=True)

    normalized_win_probability = float(win_probability)
    if (
        not math.isfinite(normalized_win_probability)
        or not 0.0 <= normalized_win_probability <= 1.0
    ):
        raise ValueError("Maia-2 returned an invalid win probability")
    return {
        "moves": candidates[:limit],
        "winProbability": normalized_win_probability,
    }


class MaiaPredictor:
    def __init__(self) -> None:
        import torch
        from maia2 import inference, model

        torch.set_num_threads(TORCH_INTRAOP_THREADS)
        torch.set_num_interop_threads(TORCH_INTEROP_THREADS)
        # Unsupported x86 hosts otherwise retry NNPACK initialization per convolution.
        torch.backends.nnpack.set_flags(False)
        self._torch = torch
        self._inference = inference
        model_type = os.getenv("MAIA_MODEL_TYPE", "rapid")
        self.identity = RuntimeIdentity(
            package=f"maia2=={version('maia2')}",
            model=model_type,
        )
        model_root = os.getenv("MAIA_MODEL_DIR", "/models")
        model_preparation = prepare_model_files(model_root)
        self._model = model.from_pretrained(
            type=model_type,
            device=os.getenv("MAIA_DEVICE", "cpu"),
            save_root=model_root,
        )
        model_source = finalize_model_files(model_preparation)
        print(f"Maia model objects loaded from {model_source}", flush=True)
        self._prepared = inference.prepare()
        self._capacity = BoundedSemaphore(MAX_CONCURRENT_PREDICTIONS)

    def predict(self, request: PredictionRequest) -> dict[str, object]:
        with self._capacity:
            with self._torch.inference_mode():
                move_probabilities, win_probability = self._inference.inference_each(
                    self._model,
                    self._prepared,
                    request.position,
                    request.player_elo,
                    request.opponent_elo,
                )
        return format_prediction(move_probabilities, win_probability, request.limit)


def handler_for(predictor: MaiaPredictor):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            self._request_started = time.perf_counter()
            if self.path == "/health":
                identity = predictor.identity
                self.write_json(
                    200,
                    {
                        "ok": True,
                        "package": identity.package,
                        "model": identity.model,
                    },
                )
            else:
                self.write_json(404, {"error": "not found"})

        def do_POST(self) -> None:
            self._request_started = time.perf_counter()
            if self.path != "/v1/predict":
                self.write_json(404, {"error": "not found"})
                return

            try:
                content_length = int(self.headers.get("Content-Length", "0"))
                if content_length <= 0 or content_length > 65_536:
                    raise ValueError("request body must be between 1 and 65536 bytes")
                payload = json.loads(self.rfile.read(content_length))
                request = parse_request(payload)
            except (ValueError, json.JSONDecodeError) as error:
                self.write_json(400, {"error": str(error)})
                return

            try:
                prediction = predictor.predict(request)
            except Exception as error:
                emit_service_log(
                    "error",
                    "maia_inference_failed",
                    errorType=type(error).__name__,
                )
                self.write_json(503, {"error": "Maia-2 inference unavailable"})
                return

            self.write_json(200, prediction)

        def write_json(self, status: int, payload: dict[str, object]) -> None:
            body = json.dumps(payload, separators=(",", ":")).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            emit_service_log(
                "error" if status >= 500 else "info",
                "maia_http_request",
                method=self.command,
                path=self.path,
                status=status,
                durationMilliseconds=round(
                    (time.perf_counter() - self._request_started) * 1000,
                    3,
                ),
            )

        def log_message(self, format: str, *args: object) -> None:
            # Request completion is emitted above with explicit severity and timing.
            pass

    return Handler


def server_for(address: tuple[str, int], predictor) -> ThreadingHTTPServer:
    return ThreadingHTTPServer(address, handler_for(predictor))


def main() -> None:
    predictor = MaiaPredictor()
    port = int(os.getenv("PORT", os.getenv("MAIA_PORT", "8080")))
    server = server_for(("0.0.0.0", port), predictor)
    print(
        f"Maia-2 service listening on 0.0.0.0:{port}; "
        f"predictionConcurrency={MAX_CONCURRENT_PREDICTIONS}; "
        f"torchThreads={TORCH_INTRAOP_THREADS}; "
        f"torchInteropThreads={TORCH_INTEROP_THREADS}",
        flush=True,
    )
    server.serve_forever()


if __name__ == "__main__":
    main()
