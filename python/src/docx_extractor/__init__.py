"""Python wrapper for the docx-extractor Rust binary.

Public API:

    docx_extractor.extract(path, **kwargs) -> dict | None
    docx_extractor.DocxExtractorError

The wheel ships a prebuilt binary in `docx_extractor/bin/` for the matching
platform. The wrapper invokes it via subprocess and parses its stdout as JSON.
"""

from __future__ import annotations

import json
from typing import Any, Optional

from ._runner import run as _run

__all__ = ["extract", "DocxExtractorError", "__version__"]

__version__ = "0.4.0"


class DocxExtractorError(RuntimeError):
    """Raised when the docx-extractor binary exits with a non-zero status.

    Attributes:
        returncode: The binary's exit code.
        stderr: Captured stderr text (already decoded as UTF-8).
    """

    def __init__(self, returncode: int, stderr: str) -> None:
        msg = stderr.strip() or f"docx-extractor exited with code {returncode}"
        super().__init__(msg)
        self.returncode = returncode
        self.stderr = stderr


def extract(
    path: str,
    *,
    pretty: bool = False,
    output: Optional[str] = None,
    no_images: bool = False,
    max_image_bytes: Optional[int] = None,
    timeout: Optional[float] = None,
) -> Optional[dict[str, Any]]:
    """Extract structured JSON from a `.docx` file.

    Args:
        path: Path to the `.docx` file to extract.
        pretty: Pretty-print JSON. Only meaningful with `output`; the in-memory
            return value is a `dict` regardless.
        output: If set, write the JSON to this path and return `None` instead
            of loading it into memory. Use this for large documents where the
            full JSON would be megabytes.
        no_images: Skip base64-encoded image bytes. Per-section `images`
            references are preserved either way. Strongly recommended for
            chat / LLM workflows where base64 dominates token cost.
        max_image_bytes: Per-image size cap in bytes. Images larger than this
            are skipped with a stderr warning. Default (when `None`): 10 MiB,
            as defined by the binary itself.
        timeout: Subprocess timeout in seconds. `None` waits indefinitely.

    Returns:
        Parsed JSON document as a `dict`, or `None` when `output` is given.

    Raises:
        DocxExtractorError: If the binary exits non-zero. The exception's
            message contains the stderr text.
        subprocess.TimeoutExpired: If `timeout` elapses.
    """
    args: list[str] = []
    if pretty:
        args.append("--pretty")
    if output is not None:
        args.extend(["--output", output])
    if no_images:
        args.append("--no-images")
    if max_image_bytes is not None:
        args.extend(["--max-image-bytes", str(max_image_bytes)])
    args.append(path)

    result = _run(args, timeout=timeout)
    if result.returncode != 0:
        raise DocxExtractorError(result.returncode, result.stderr)

    if output is not None:
        return None
    return json.loads(result.stdout)
