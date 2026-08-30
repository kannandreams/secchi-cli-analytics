"""Privacy-safe local analytics adapters for Python CLIs."""

from importlib.metadata import PackageNotFoundError, version

try:
    __version__ = version("secchi-analytics")
except PackageNotFoundError:
    __version__ = "0.1.0"

__all__ = ["__version__"]
