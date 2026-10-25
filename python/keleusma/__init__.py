"""Run Keleusma scripts from Python.

The public interface exposes version information only so far. The planned interface
is in docs/architecture/BINDING_DESIGN.md.
"""

from keleusma._keleusma import BYTECODE_VERSION, KELEUSMA_VERSION, __version__

__all__ = ["BYTECODE_VERSION", "KELEUSMA_VERSION", "__version__"]
