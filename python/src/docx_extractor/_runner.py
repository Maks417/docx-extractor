"""Subprocess wrapper around the bundled docx-extractor binary.

Mirrors the design of mcp/src/runner.ts in the npm package — capture stdout
and stderr separately, return a small result struct, surface non-zero exits
to the caller.
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from typing import Optional, Sequence

from ._binary import path as _binary_path

__all__ = ["RunResult", "run"]


@dataclass(frozen=True)
class RunResult:
    """Outcome of a single binary invocation."""

    stdout: str
    stderr: str
    returncode: int


def run(args: Sequence[str], *, timeout: Optional[float] = None) -> RunResult:
    """Invoke the bundled docx-extractor binary with `args` and return its
    captured stdout/stderr/exit code.

    Stdin is closed; stdout and stderr are captured as UTF-8 text. Errors from
    the binary are *not* raised here — callers (e.g. `docx_extractor.extract`)
    decide how to react to non-zero return codes.

    Raises:
        subprocess.TimeoutExpired: If `timeout` elapses.
        FileNotFoundError: If the bundled binary is missing.
    """
    completed = subprocess.run(
        [_binary_path(), *args],
        stdin=subprocess.DEVNULL,
        capture_output=True,
        timeout=timeout,
        # Decode with replacement so a malformed UTF-8 byte in stderr never
        # turns a docx error into a Python UnicodeDecodeError.
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    return RunResult(
        stdout=completed.stdout or "",
        stderr=completed.stderr or "",
        returncode=completed.returncode,
    )
