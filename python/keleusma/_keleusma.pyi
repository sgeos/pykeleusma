"""Type stub for the compiled extension module."""

from typing import Literal, final

__version__: str
KELEUSMA_VERSION: str
BYTECODE_VERSION: int
DEFAULT_ARGUMENT_BYTES: int

@final
class Module:
    """A compiled Keleusma module. Immutable."""

def compile(source: str) -> Module:
    """Compile Keleusma source text into a module.

    Raises `keleusma.LexError`, `keleusma.ParseError` or `keleusma.CompileError`.
    """

@final
class VM:
    """A verified Keleusma VM that owns its arena. Belongs to the creating thread."""

    def __new__(cls, module: Module, *, argument_bytes: int = ...) -> VM: ...
    @property
    def arena_capacity(self) -> int: ...
    @property
    def argument_bytes(self) -> int: ...
    def _call_no_args(
        self,
    ) -> tuple[Literal["yielded", "finished", "reset"], int | None]: ...

def _live_vm_count() -> int: ...
