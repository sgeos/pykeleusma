"""Run Keleusma scripts from Python.

`compile` turns source text into a `Module`, raising a `SourceError` subclass with the
line and column of the fault. `VM(module)` verifies a module and builds a VM with its
own arena, raising `VerifyError` if the verifier rejects it. Values convert by the
entry point's declared types, with `Fixed` for fixed-point numbers. `vm.call(*args)` and
`vm.resume(value)` return `Yielded`, `Finished` or `Reset`. The planned interface is in
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
from keleusma._fixed import Fixed
from keleusma._state import Finished, Reset, State, Yielded
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
    "Finished",
    "Fixed",
    "KeleusmaError",
    "LexError",
    "Module",
    "ParseError",
    "Reset",
    "SourceError",
    "State",
    "VMError",
    "VerifyError",
    "Yielded",
    "__version__",
    "compile",
]
