from contextlib import nullcontext
from hashlib import sha256
from io import BytesIO
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

from model_artifacts import (
    download_model_object,
    finalize_model_files,
    prepare_model_files,
    upload_model_object,
)


class ModelArtifactStoreTest(unittest.TestCase):
    def test_reuses_verified_model_volume_before_remote_store(self):
        with TemporaryDirectory() as temporary_directory:
            model_root = Path(temporary_directory) / "model-cache"
            model_root.mkdir()
            for name in ("rapid_model.pt", "config.yaml"):
                (model_root / name).write_text("persisted")

            with (
                patch.dict(
                    "model_artifacts.os.environ",
                    {
                        "MAIA_MODEL_STORE_URL": "http://store.test/internal/models",
                        "ARTIFACT_STORE_TOKEN": "test token",
                    },
                    clear=False,
                ),
                patch(
                    "model_artifacts.verify_model_objects"
                ) as verify_model_objects,
                patch(
                    "model_artifacts.download_model_object"
                ) as download_model,
            ):
                preparation = prepare_model_files(str(model_root))

        verify_model_objects.assert_called_once_with(model_root)
        download_model.assert_not_called()
        self.assertEqual(
            finalize_model_files(preparation),
            "verified persistent model volume",
        )

    def test_remote_cache_miss_falls_back_without_partial_files(self):
        with TemporaryDirectory() as temporary_directory:
            model_root = Path(temporary_directory)
            (model_root / "rapid_model.pt").write_text("partial")
            with (
                patch.dict(
                    "model_artifacts.os.environ",
                    {
                        "MAIA_MODEL_STORE_URL": "http://store.test/internal/models",
                        "ARTIFACT_STORE_TOKEN": "test token",
                    },
                    clear=False,
                ),
                patch(
                    "model_artifacts.download_model_object",
                    return_value=False,
                ),
            ):
                preparation = prepare_model_files(str(model_root))

            self.assertEqual(preparation.source, "upstream Maia package cache")
            self.assertTrue(preparation.needs_package_config)
            self.assertFalse((model_root / "rapid_model.pt").exists())
            self.assertFalse((model_root / "config.yaml").exists())

    def test_package_cache_config_is_persisted_after_model_load(self):
        model_contents = b"model contents"
        config_contents = b"packaged config"
        with TemporaryDirectory() as temporary_directory:
            model_root = Path(temporary_directory) / "model-cache"
            packaged_config = Path(temporary_directory) / "maia2-training.yaml"
            packaged_config.write_bytes(config_contents)
            with (
                patch.dict(
                    "model_artifacts.os.environ",
                    {},
                    clear=True,
                ),
                patch.dict(
                    "model_artifacts.MODEL_DIGESTS",
                    {
                        "rapid_model.pt": sha256(model_contents).hexdigest(),
                        "config.yaml": sha256(config_contents).hexdigest(),
                    },
                    clear=True,
                ),
                patch(
                    "model_artifacts.files",
                    return_value=MockPackageFiles(packaged_config),
                ),
                patch(
                    "model_artifacts.as_file",
                    side_effect=lambda resource: nullcontext(resource),
                ),
            ):
                preparation = prepare_model_files(str(model_root))
                (model_root / "rapid_model.pt").write_bytes(model_contents)
                source = finalize_model_files(preparation)

            self.assertEqual(
                source,
                "upstream Maia package cache persisted locally",
            )
            self.assertEqual(
                (model_root / "config.yaml").read_bytes(),
                config_contents,
            )

    def test_download_is_digest_checked_and_atomically_published(self):
        contents = b"model contents"
        expected = sha256(contents).hexdigest()
        with TemporaryDirectory() as temporary_directory:
            destination = Path(temporary_directory) / "rapid_model.pt"
            with (
                patch.dict(
                    "model_artifacts.MODEL_DIGESTS",
                    {"rapid_model.pt": expected},
                    clear=True,
                ),
                patch(
                    "model_artifacts.urlopen",
                    return_value=BytesIO(contents),
                ),
            ):
                downloaded = download_model_object(
                    "http://store.test/internal/models",
                    "test token",
                    "rapid_model.pt",
                    destination,
                )

            self.assertTrue(downloaded)
            self.assertEqual(destination.read_bytes(), contents)
            self.assertEqual(list(destination.parent.glob(".rapid_model.pt.*")), [])

    def test_remote_cache_miss_is_streamed_back_after_model_load(self):
        model_contents = b"model contents"
        config_contents = b"packaged config"
        with TemporaryDirectory() as temporary_directory:
            model_root = Path(temporary_directory) / "model-cache"
            packaged_config = Path(temporary_directory) / "maia2-training.yaml"
            packaged_config.write_bytes(config_contents)
            with (
                patch.dict(
                    "model_artifacts.os.environ",
                    {
                        "MAIA_MODEL_STORE_URL": "http://store.test/internal/models",
                        "ARTIFACT_STORE_TOKEN": "test token",
                    },
                    clear=True,
                ),
                patch.dict(
                    "model_artifacts.MODEL_DIGESTS",
                    {
                        "rapid_model.pt": sha256(model_contents).hexdigest(),
                        "config.yaml": sha256(config_contents).hexdigest(),
                    },
                    clear=True,
                ),
                patch(
                    "model_artifacts.download_model_object",
                    return_value=False,
                ),
                patch(
                    "model_artifacts.files",
                    return_value=MockPackageFiles(packaged_config),
                ),
                patch(
                    "model_artifacts.as_file",
                    side_effect=lambda resource: nullcontext(resource),
                ),
                patch("model_artifacts.upload_model_object") as upload,
            ):
                preparation = prepare_model_files(str(model_root))
                (model_root / "rapid_model.pt").write_bytes(model_contents)
                source = finalize_model_files(preparation)

            self.assertEqual(source, "upstream seed persisted to artifact store")
            self.assertEqual(
                [call.args[2] for call in upload.call_args_list],
                ["rapid_model.pt", "config.yaml"],
            )

    def test_model_upload_streams_file_chunks(self):
        contents = b"model contents"
        with TemporaryDirectory() as temporary_directory:
            source = Path(temporary_directory) / "rapid_model.pt"
            source.write_bytes(contents)

            def accept_upload(request, timeout):
                self.assertEqual(timeout, 300)
                self.assertNotIsInstance(request.data, bytes)
                self.assertEqual(b"".join(request.data), contents)
                self.assertEqual(
                    request.get_header("Content-length"),
                    str(len(contents)),
                )
                return BytesIO()

            with (
                patch.dict(
                    "model_artifacts.MODEL_DIGESTS",
                    {"rapid_model.pt": sha256(contents).hexdigest()},
                    clear=True,
                ),
                patch("model_artifacts.urlopen", side_effect=accept_upload),
            ):
                upload_model_object(
                    "http://store.test/internal/models",
                    "test token",
                    "rapid_model.pt",
                    source,
                )


class MockPackageFiles:
    def __init__(self, config_path: Path):
        self.config_path = config_path

    def joinpath(self, name: str) -> Path:
        if name != "maia2-training.yaml":
            raise AssertionError(f"unexpected packaged resource: {name}")
        return self.config_path


if __name__ == "__main__":
    unittest.main()
