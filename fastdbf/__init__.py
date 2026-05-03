from . import fastdbf
from .fastdbf import *  # noqa: F403
from .fastdbf import __author__, __version__

__doc__ = fastdbf.__doc__
if hasattr(fastdbf, "__all__"):
    __all__ = [*fastdbf.__all__, "__version__", "__author__"]
else:
    __all__ = ["__version__", "__author__"]
