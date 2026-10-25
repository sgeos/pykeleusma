"""Type stub for the compiled extension module."""

from typing import Literal, final

__version__: str
KELEUSMA_VERSION: str
BYTECODE_VERSION: int

@final
class Module:
    """A compiled Keleusma module. Immutable."""

def compile(source: str) -> Module:
    """Compile Keleusma source text into a module.

    Raises `keleusma.LexError`, `keleusma.ParseError` or `keleusma.CompileError`.
    """

@final
class _OwnedVmProbe:
    """A VM owned together with its arena. Temporary, removed in roadmap task 3."""

    def __new__(cls, source: str) -> _OwnedVmProbe: ...
    @property
    def arena_capacity(self) -> int: ...
    def call(self) -> tuple[Literal["yielded", "finished", "reset"], int | None]: ...

def _live_vm_count() -> int: ...
