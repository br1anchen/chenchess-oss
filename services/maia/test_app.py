from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
import json
import sys
import unittest
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier, Lock, Thread
from types import ModuleType, SimpleNamespace
from unittest.mock import Mock, patch

from app import (
    MAX_CONCURRENT_PREDICTIONS,
    MaiaPredictor,
    RuntimeIdentity,
    emit_service_log,
    format_prediction,
    parse_request,
    server_for,
)


PREDICTION_BODY = (
    b'{"position":"8/8/8/8/8/8/8/K6k w - - 0 1",'
    b'"playerElo":1200,"opponentElo":1200,"limit":3}'
)


class MaiaServiceContractTest(unittest.TestCase):
    def test_service_log_routes_success_to_structured_stdout(self):
        standard_output = StringIO()
        standard_error = StringIO()

        with (
            redirect_stdout(standard_output),
            redirect_stderr(standard_error),
        ):
            emit_service_log("info", "maia_http_request", status=200)

        self.assertEqual(standard_error.getvalue(), "")
        self.assertEqual(
            json.loads(standard_output.getvalue()),
            {
                "level": "info",
                "event": "maia_http_request",
                "status": 200,
            },
        )

    def test_disables_nnpack_before_loading_model(self):
        events = []
        nnpack = SimpleNamespace(
            set_flags=Mock(
                side_effect=lambda enabled: events.append(("nnpack", enabled))
            )
        )
        fake_torch = ModuleType("torch")
        fake_torch.backends = SimpleNamespace(nnpack=nnpack)
        fake_torch.set_num_threads = Mock()
        fake_torch.set_num_interop_threads = Mock()

        fake_inference = SimpleNamespace(prepare=Mock(return_value=object()))
        fake_model = SimpleNamespace(
            from_pretrained=Mock(
                side_effect=lambda **_: (
                    events.append(("model", "loaded")),
                    object(),
                )[1]
            )
        )
        fake_maia2 = ModuleType("maia2")
        fake_maia2.inference = fake_inference
        fake_maia2.model = fake_model

        with (
            patch.dict(sys.modules, {"torch": fake_torch, "maia2": fake_maia2}),
            patch("app.prepare_model_files", return_value=object()),
            patch("app.finalize_model_files", return_value="test cache"),
            patch("app.version", return_value="0.11.0"),
        ):
            MaiaPredictor()

        self.assertEqual(
            events[:2],
            [("nnpack", False), ("model", "loaded")],
        )

    def test_health_reports_separate_package_and_model_identities(self):
        class HealthyPredictor:
            identity = RuntimeIdentity(package="maia2==0.11.0", model="rapid")

        server = server_for(("127.0.0.1", 0), HealthyPredictor())
        thread = Thread(target=server.serve_forever)
        thread.start()
        standard_output = StringIO()
        standard_error = StringIO()
        health = None
        try:
            with (
                redirect_stdout(standard_output),
                redirect_stderr(standard_error),
                urllib.request.urlopen(
                    f"http://127.0.0.1:{server.server_port}/health"
                ) as response,
            ):
                health = json.load(response)
        finally:
            server.shutdown()
            thread.join()
            server.server_close()

        self.assertEqual(
            health,
            {
                "ok": True,
                "package": "maia2==0.11.0",
                "model": "rapid",
            },
        )
        self.assertEqual(standard_error.getvalue(), "")
        access_log = json.loads(standard_output.getvalue())
        self.assertEqual(access_log["level"], "info")
        self.assertEqual(access_log["event"], "maia_http_request")
        self.assertEqual(access_log["status"], 200)

    def test_parses_position_and_elo_contract(self):
        request = parse_request(
            {
                "position": "8/8/8/8/8/8/8/K6k w - - 0 1",
                "playerElo": 1450,
                "opponentElo": 1450,
                "limit": 3,
            }
        )

        self.assertEqual(request.player_elo, 1450)
        self.assertEqual(request.limit, 3)

    def test_rejects_invalid_elo(self):
        with self.assertRaisesRegex(ValueError, "playerElo"):
            parse_request(
                {
                    "position": "8/8/8/8/8/8/8/K6k w - - 0 1",
                    "playerElo": 99,
                    "opponentElo": 1200,
                    "limit": 3,
                }
            )

    def test_formats_ranked_probability_candidates(self):
        response = format_prediction(
            {"d2d4": 0.31, "e2e4": 0.46, "g1f3": 0.12},
            0.53,
            limit=2,
        )

        self.assertEqual(
            response["moves"],
            [
                {"uci": "e2e4", "probability": 0.46},
                {"uci": "d2d4", "probability": 0.31},
            ],
        )
        self.assertEqual(response["winProbability"], 0.53)

    def test_rejects_empty_model_output(self):
        with self.assertRaisesRegex(ValueError, "no move candidates"):
            format_prediction({}, 0.5, limit=3)

    def test_inference_value_error_is_a_recoverable_service_failure(self):
        class FailingPredictor:
            def predict(self, request):
                raise ValueError("invalid model output")

        server = server_for(("127.0.0.1", 0), FailingPredictor())
        thread = Thread(target=server.serve_forever)
        thread.start()
        request = urllib.request.Request(
            f"http://127.0.0.1:{server.server_port}/v1/predict",
            data=PREDICTION_BODY,
            headers={"Content-Type": "application/json"},
            method="POST",
        )

        try:
            with self.assertRaises(urllib.error.HTTPError) as failure:
                urllib.request.urlopen(request)
            self.assertEqual(failure.exception.code, 503)
        finally:
            server.shutdown()
            thread.join()
            server.server_close()

    def test_serves_four_prediction_requests_concurrently(self):
        class ConcurrentPredictor:
            def __init__(self):
                self.barrier = Barrier(MAX_CONCURRENT_PREDICTIONS)
                self.lock = Lock()
                self.active = 0
                self.peak = 0

            def predict(self, request):
                with self.lock:
                    self.active += 1
                    self.peak = max(self.peak, self.active)
                try:
                    self.barrier.wait(timeout=2)
                    return {
                        "moves": [{"uci": "a1a2", "probability": 0.5}],
                        "winProbability": 0.5,
                    }
                finally:
                    with self.lock:
                        self.active -= 1

        predictor = ConcurrentPredictor()
        server = server_for(("127.0.0.1", 0), predictor)
        thread = Thread(target=server.serve_forever)
        thread.start()

        def predict():
            request = urllib.request.Request(
                f"http://127.0.0.1:{server.server_port}/v1/predict",
                data=PREDICTION_BODY,
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            with urllib.request.urlopen(request, timeout=3) as response:
                return response.status

        try:
            with ThreadPoolExecutor(max_workers=MAX_CONCURRENT_PREDICTIONS) as executor:
                statuses = executor.map(
                    lambda _: predict(), range(MAX_CONCURRENT_PREDICTIONS)
                )
                self.assertEqual(list(statuses), [200] * MAX_CONCURRENT_PREDICTIONS)
            self.assertEqual(predictor.peak, MAX_CONCURRENT_PREDICTIONS)
        finally:
            server.shutdown()
            thread.join()
            server.server_close()


if __name__ == "__main__":
    unittest.main()
