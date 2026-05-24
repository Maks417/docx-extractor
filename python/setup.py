"""Thin setup.py used only so the wheel build can be invoked as:

    python -m build --wheel --config-setting=--build-option=--plat-name=<tag>

Everything else (metadata, dependencies, console scripts) lives in
pyproject.toml. We need this file because `bdist_wheel --plat-name` is not yet
configurable via PEP 517 config settings in a portable way.

The wheel is *not* pure-python: it ships a prebuilt platform-specific binary in
`src/docx_extractor/bin/`. We force-tag it as a platform wheel by overriding
`has_ext_modules` on the distribution.
"""

from setuptools import setup
from setuptools.dist import Distribution


class BinaryDistribution(Distribution):
    """Force setuptools to build a platform-specific wheel even though there
    are no compiled Python extensions in this project — the bundled native
    binary lives in package_data."""

    def has_ext_modules(self) -> bool:  # type: ignore[override]
        return True

    def is_pure(self) -> bool:
        return False


setup(distclass=BinaryDistribution)
