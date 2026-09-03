# Local Pipeline Runtime third-party notices

The Local Pipeline Runtime combines separately licensed components. The notices below travel with the installed runtime and inside the Maia container. They do not change the license of chenchess itself.

## Stockfish

Stockfish 18 is licensed under GNU GPL version 3. The installer downloads the official Apple Silicon archive, verifies its pinned checksum, and installs the archive's `Copying.txt` beside the binary. Source and license information: <https://stockfishchess.org/download/>.

## Maia-2 and model artifacts

The Maia-2 Python package (`maia2==0.11.0`) is distributed under the MIT License by the CSSLab Maia-2 project. Its license text is included as `MAIA2-MIT.txt` in the container. Source: <https://github.com/CSSLab/maia2>.

The pinned `rapid_model.pt` artifact is obtained by Maia-2's `from_pretrained` API, and the matching packaged `config.yaml` is copied into the model volume. The upstream Maia-2 distribution identifies both artifacts under the same MIT license. The installer records their model identity and verifies both checksums before activation.

## Python container and packages

The Maia service container is derived from the digest-pinned official Python 3.11 slim image. Python is distributed under the Python Software Foundation License; the base image retains its operating-system package copyright files. Python package metadata and license files installed by `pip`, including PyTorch and Maia-2 dependencies, remain in their `.dist-info` directories inside the image. Image source: <https://github.com/docker-library/python>.

The corresponding source projects and exact dependency versions are declared in `services/maia/Dockerfile` and `services/maia/requirements.txt`. Redistributors should preserve the image layers, this notice, the bundled Maia-2 license, the Stockfish `Copying.txt`, and all package metadata.
