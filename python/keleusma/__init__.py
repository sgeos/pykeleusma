"""Run Keleusma scripts from Python.

`compile` turns source text into a `Module`, raising a `SourceError` subclass with the
line and column of the fault. `VM(module)` verifies a module and builds a VM with its
own arena, raising `VerifyError` if the verifier rejects it. Calling and resuming a VM
arrive with later roadmap tasks. The planned interface is in
docs/architecture/BINDING_DESIGN.md.
"""

from keleusma._errors import (
    CompileError,
    KeleusmaError,
    LexError,
    ParseError,
    SourceError,
    VerifyError,
    VMError,
)
from keleusma._keleusma import (
    BYTECODE_VERSION,
    DEFAULT_ARGUMENT_BYTES,
    KELEUSMA_VERSION,
    VM,
    Module,
    __version__,
    compile,  # Like re.compile, it shadows the builtin only inside this package.
)

__all__ = [
    "BYTECODE_VERSION",
    "KELEUSMA_VERSION",
    "DEFAULT_ARGUMENT_BYTES",
    "VM",
    "CompileError",
    "KeleusmaError",
    "LexError",
    "Module",
    "ParseError",
    "SourceError",
    "VMError",
    "VerifyError",
    "__version__",
    "compile",
]
