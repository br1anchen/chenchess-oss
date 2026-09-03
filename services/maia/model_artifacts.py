from __future__ import annotations

import hashlib
import os
from collections.abc import Iterator
from dataclasses import dataclass
from importlib.resources import as_file, files
from pathlib import Path
from tempfile import NamedTemporaryFile
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen


MODEL_DIGESTS = {
    "rapid_model.pt": "65aae8465eed5e65df66a24ea7370715579f9e5435098d06fe18bdb1e267e997",
    "config.yaml": "4b06a5e6917dba8a55defaf3947ce97a73edca3ae2c9d225779a620353c1371b",
}


@dataclass(frozen=True)
class ModelArtifactPreparation:
    root: Path
    source: str
    needs_package_config: bool
    store_url: str | None = None
    store_token: str | None = None


def prepare_model_files(model_root: str) -> ModelArtifactPreparation:
    root = Path(model_root)
    root.mkdir(parents=True, exist_ok=True)
    if has_verified_model_objects(root):
        return ModelArtifactPreparation(
            root,
            "verified persistent model volume",
            needs_package_config=False,
        )
    remove_model_objects(root)

    store_url = os.getenv("MAIA_MODEL_STORE_URL")
    store_token = os.getenv("ARTIFACT_STORE_TOKEN")
    if not store_url and not store_token:
        return package_cache_preparation(root)
    if not store_url or not store_token:
        raise ValueError(
            "MAIA_MODEL_STORE_URL and ARTIFACT_STORE_TOKEN must be set together"
        )

    try:
        for name in MODEL_DIGESTS:
            if not download_model_object(store_url, store_token, name, root / name):
                remove_model_objects(root)
                return package_cache_preparation(root, store_url, store_token)
        verify_model_objects(root)
    except Exception:
        remove_model_objects(root)
        raise
    return ModelArtifactPreparation(
        root,
        "Cloud Storage bootstrap",
        needs_package_config=False,
    )


def finalize_model_files(preparation: ModelArtifactPreparation) -> str:
    if not preparation.needs_package_config:
        return preparation.source
    persist_package_config(preparation.root)
    verify_model_objects(preparation.root)
    if preparation.store_url is not None and preparation.store_token is not None:
        try:
            for name in MODEL_DIGESTS:
                upload_model_object(
                    preparation.store_url,
                    preparation.store_token,
                    name,
                    preparation.root / name,
                )
        except URLError as error:
            if not isinstance(error.reason, BrokenPipeError):
                raise
            return "upstream seed retained locally after artifact store closed upload"
        return "upstream seed persisted to artifact store"
    return "upstream Maia package cache persisted locally"


def package_cache_preparation(
    root: Path,
    store_url: str | None = None,
    store_token: str | None = None,
) -> ModelArtifactPreparation:
    return ModelArtifactPreparation(
        root,
        "upstream Maia package cache",
        needs_package_config=True,
        store_url=store_url,
        store_token=store_token,
    )


def persist_package_config(root: Path) -> None:
    packaged = files("maia2.configs").joinpath("maia2-training.yaml")
    with as_file(packaged) as source:
        publish_verified_file(
            Path(source),
            root / "config.yaml",
            MODEL_DIGESTS["config.yaml"],
        )


def publish_verified_file(source: Path, destination: Path, expected: str) -> None:
    temporary_path = None
    try:
        with (
            source.open("rb") as source_file,
            NamedTemporaryFile(
                mode="wb",
                dir=destination.parent,
                prefix=f".{destination.name}.",
                delete=False,
            ) as temporary,
        ):
            temporary_path = Path(temporary.name)
            digest = hashlib.sha256()
            while chunk := source_file.read(1024 * 1024):
                temporary.write(chunk)
                digest.update(chunk)
            temporary.flush()
            os.fsync(temporary.fileno())
        if digest.hexdigest() != expected:
            raise ValueError(f"{destination.name} digest mismatch")
        os.replace(temporary_path, destination)
    finally:
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)


def download_model_object(
    store_url: str, token: str, name: str, destination: Path
) -> bool:
    request = Request(
        f"{store_url.rstrip('/')}/{quote(name)}",
        headers={"Authorization": f"Bearer {token}"},
    )
    temporary_path = None
    try:
        try:
            with urlopen(request, timeout=300) as response:
                with NamedTemporaryFile(
                    mode="wb",
                    dir=destination.parent,
                    prefix=f".{destination.name}.",
                    delete=False,
                ) as temporary:
                    temporary_path = Path(temporary.name)
                    digest = hashlib.sha256()
                    while chunk := response.read(1024 * 1024):
                        temporary.write(chunk)
                        digest.update(chunk)
                    temporary.flush()
                    os.fsync(temporary.fileno())
        except HTTPError as error:
            if error.code == 404:
                return False
            raise
        if digest.hexdigest() != MODEL_DIGESTS[name]:
            raise ValueError(f"{name} digest mismatch")
        os.replace(temporary_path, destination)
        return True
    finally:
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)


def upload_model_object(
    store_url: str, token: str, name: str, source: Path
) -> None:
    request = Request(
        f"{store_url.rstrip('/')}/{quote(name)}",
        data=file_chunks(source),
        method="PUT",
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/octet-stream",
            "Content-Length": str(source.stat().st_size),
            "X-Content-SHA256": MODEL_DIGESTS[name],
        },
    )
    with urlopen(request, timeout=300):
        pass


def file_chunks(source: Path) -> Iterator[bytes]:
    with source.open("rb") as source_file:
        while chunk := source_file.read(1024 * 1024):
            yield chunk


def has_verified_model_objects(root: Path) -> bool:
    if not all((root / name).is_file() for name in MODEL_DIGESTS):
        return False
    try:
        verify_model_objects(root)
    except (OSError, ValueError):
        return False
    return True


def remove_model_objects(root: Path) -> None:
    for name in MODEL_DIGESTS:
        (root / name).unlink(missing_ok=True)


def verify_model_objects(root: Path) -> None:
    for name, expected in MODEL_DIGESTS.items():
        verify_digest(root / name, expected)


def verify_digest(path: Path, expected: str) -> None:
    digest = hashlib.sha256()
    with path.open("rb") as model_file:
        while chunk := model_file.read(1024 * 1024):
            digest.update(chunk)
    actual = digest.hexdigest()
    if actual != expected:
        raise ValueError(f"{path.name} digest mismatch")
