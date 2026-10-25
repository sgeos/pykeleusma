"""Run Keleusma scripts from Python.

`compile` turns source text into a `Module`, raising a `SourceError` subclass with the
line and column of the fault. `VM(module)` verifies a module and builds a VM with its
own arena, raising `VerifyError` if the verifier rejects it. Values convert by the
entry point's declared types, with `Fixed` for fixed-point numbers. `vm.call(*args)` and
`vm.resume(value)` return `Yielded`, `Finished` or `Reset`, and `vm.stream` drives them
as a generator. Python callables serve as natives through `natives=` or `vm.register`,
with bounds that Keleusma trusts but Python cannot honour. The planned interface is in
docs/architecture/BINDING_DESIGN.md.
"""

from importlib.metadata import version as _distribution_version

from keleusma._errors import (
    CompileError,
    KeleusmaError,
    LexError,
    LoadError,
    NativeError,
    ParseError,
    SourceError,
    VerifyError,
    VMError,
)
from keleusma._fixed import Fixed
from keleusma._native import DEFAULT_NATIVE_WCET, DEFAULT_NATIVE_WCMU_BYTES, Native
from keleusma._state import Finished, Reset, State, Yielded
from keleusma._values import Variant
from keleusma._keleusma import (
    BYTECODE_VERSION,
    DEFAULT_ARGUMENT_BYTES,
    KELEUSMA_VERSION,
    OP_NAMES,
    VM,
    Module,
    library_signatures,
    load_module,
    compile,  # Like re.compile, it shadows the builtin only inside this package.
    encryption_public_key,
    signing_public_key,
)

# Read from the installed distribution, because pyproject.toml holds the version and
# Cargo cannot carry its four parts.
__version__ = _distribution_version("pykeleusma")

__all__ = [
    "BYTECODE_VERSION",
    "KELEUSMA_VERSION",
    "DEFAULT_ARGUMENT_BYTES",
    "DEFAULT_NATIVE_WCET",
    "DEFAULT_NATIVE_WCMU_BYTES",
    "VM",
    "CompileError",
    "Finished",
    "Fixed",
    "KeleusmaError",
    "LexError",
    "LoadError",
    "NativeError",
    "OP_NAMES",
    "Native",
    "Module",
    "ParseError",
    "Reset",
    "SourceError",
    "State",
    "VMError",
    "Variant",
    "VerifyError",
    "Yielded",
    "__version__",
    "compile",
    "encryption_public_key",
    "library_signatures",
    "load_module",
    "signing_public_key",
]
