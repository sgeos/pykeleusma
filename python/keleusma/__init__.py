"""Run Keleusma scripts from Python.

`compile` turns source text into a `Module`, raising a `SourceError` subclass with the
line and column of the fault. Running a module arrives with later roadmap tasks. The
planned interface is in docs/architecture/BINDING_DESIGN.md.
"""

from keleusma._errors import (
    CompileError,
    KeleusmaError,
    LexError,
    ParseError,
    SourceError,
)
from keleusma._keleusma import (
    BYTECODE_VERSION,
    KELEUSMA_VERSION,
    Module,
    __version__,
    compile,  # Like re.compile, it shadows the builtin only inside this package.
)

__all__ = [
    "BYTECODE_VERSION",
    "KELEUSMA_VERSION",
    "CompileError",
    "KeleusmaError",
    "LexError",
    "Module",
    "ParseError",
    "SourceError",
    "__version__",
    "compile",
]
