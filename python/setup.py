"""Thin setup.py used only so the wheel build can be invoked as:

    python setup.py bdist_wheel --plat-name=<tag>

Everything else (metadata, dependencies, console scripts) lives in
pyproject.toml. We need this file for two reasons:

1. Force a *platform* wheel even though there are no Python C extensions —
   the bundled binary is invoked via subprocess, but the wheel is still
   platform-specific because the binary is. `BinaryDistribution` below does
   this via `has_ext_modules → True`.

2. Force the wheel's Python and ABI tags to `py3` / `none` so the wheel is
   installable on **any** Python 3 interpreter on the matching platform.
   Without this override setuptools' bdist_wheel sees has_ext_modules=True
   and defaults the ABI tag to the runner's interpreter (e.g. `cp311`),
   producing wheels that only install on that exact Python version.
   `_PlatformWheel.get_tag` below pins the tags explicitly.
"""

from setuptools import setup
from setuptools.dist import Distribution

# Modern setuptools (>= 70.1) ships bdist_wheel under setuptools.command;
# older installs need it from the `wheel` package. Try both for robustness.
try:
    from setuptools.command.bdist_wheel import bdist_wheel as _bdist_wheel
except ImportError:  # pragma: no cover - fallback for old setuptools
    from wheel.bdist_wheel import bdist_wheel as _bdist_wheel  # type: ignore[no-redef]


class BinaryDistribution(Distribution):
    """Force setuptools to build a platform-specific wheel even though there
    are no compiled Python extensions in this project — the bundled native
    binary lives in package_data."""

    def has_ext_modules(self) -> bool:  # type: ignore[override]
        return True

    def is_pure(self) -> bool:
        return False


class _PlatformWheel(_bdist_wheel):
    """Tag wheels as `py3-none-<plat>` — any Python 3, no ABI, but platform-
    specific because of the bundled binary. The platform comes from the
    --plat-name CLI argument; we just override the Python and ABI tags."""

    def get_tag(self):  # type: ignore[override]
        _python, _abi, plat = super().get_tag()
        return "py3", "none", plat


setup(
    distclass=BinaryDistribution,
    cmdclass={"bdist_wheel": _PlatformWheel},
)
