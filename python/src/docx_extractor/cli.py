"""Console-script entrypoint installed as `docx-extractor`.

A thin pass-through to the bundled binary: forwards argv, stdin, stdout,
stderr unchanged and propagates the exit code. On POSIX we use `os.execv`
so the Python process is replaced by the binary and signals/exit codes
behave exactly as if the binary were invoked directly. On Windows execv
has different semantics around argument quoting and the parent shell, so
we fall back to subprocess and forward the return code.
"""

from __future__ import annotations

import os
import subprocess
import sys

from ._binary import BinaryNotFound, path as _binary_path


def main() -> int:
    try:
        binary = _binary_path()
    except BinaryNotFound as e:
        print(str(e), file=sys.stderr)
        return 1

    args = sys.argv[1:]

    if sys.platform == "win32":
        completed = subprocess.run([binary, *args], check=False)
        return completed.returncode

    # POSIX: replace the current process so signals (SIGINT from Ctrl-C, etc.)
    # and exit codes pass through cleanly without a Python wrapper in the way.
    os.execv(binary, [binary, *args])
    # Unreachable on success; execv either replaces the process or raises.
    return 1


if __name__ == "__main__":
    sys.exit(main())
