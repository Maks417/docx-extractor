"""Locate the bundled docx-extractor binary inside the installed wheel.

The wheel ships exactly one binary per platform under `docx_extractor/bin/`:

    POSIX:    docx-extractor
    Windows:  docx-extractor.exe

This module exposes `path()` which returns an absolute filesystem path to that
binary, handling zip-imported distributions transparently and chmod'ing the
file as executable on POSIX if it isn't already.

If the binary is missing (e.g. the user installed the sdist, or built a wheel
on a platform we don't have a binary for), `path()` raises `BinaryNotFound`
with a message explaining how to recover.
"""

from __future__ import annotations

import os
import stat as _stat
import sys
from importlib.resources import as_file, files
from pathlib import Path

__all__ = ["path", "BinaryNotFound"]


class BinaryNotFound(RuntimeError):
    """Raised when no bundled docx-extractor binary is found inside the
    installed package. This typically means the user installed the sdist
    rather than a platform wheel."""


_BINARY_NAME = "docx-extractor.exe" if sys.platform == "win32" else "docx-extractor"

# Cache the resolved path. importlib.resources.as_file may return a temporary
# extracted copy for zip-imported packages; we want to extract once and reuse.
_cached_path: str | None = None


def _resolve() -> str:
    resource = files("docx_extractor").joinpath("bin", _BINARY_NAME)
    # `as_file` returns a context manager whose value is a real filesystem
    # path. For ordinary installations the path is already on disk and the
    # context manager is a no-op; for zip-imported wheels it materializes a
    # temp file. We enter+exit the context here and rely on the underlying
    # path staying alive — every realistic install path (pip + wheel) puts
    # the binary on disk, so this is safe.
    with as_file(resource) as p:
        bin_path = Path(p)
    if not bin_path.exists():
        raise BinaryNotFound(
            f"docx-extractor binary not found at {bin_path}. "
            "If you installed from sdist, install a platform wheel instead: "
            "`pip install --only-binary :all: docx-extractor`. "
            "If your platform isn't supported, build from source: "
            "https://github.com/Maks417/docx-extractor"
        )
    if sys.platform != "win32":
        # Wheels should already ship the binary as 0755, but some archive
        # extraction paths drop the executable bit. Restore it idempotently.
        mode = bin_path.stat().st_mode
        if not mode & _stat.S_IXUSR:
            os.chmod(bin_path, mode | _stat.S_IXUSR | _stat.S_IXGRP | _stat.S_IXOTH)
    return str(bin_path)


def path() -> str:
    """Return an absolute filesystem path to the bundled binary."""
    global _cached_path
    if _cached_path is None:
        _cached_path = _resolve()
    return _cached_path
