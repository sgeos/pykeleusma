"""Type stub for the compiled extension module."""

from typing import final

from keleusma._state import Finished, Reset, Yielded

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
    @property
    def failed(self) -> bool: ...
    def call(self, *args: object) -> Yielded | Finished | Reset: ...
    def resume(self, value: object) -> Yielded | Finished | Reset: ...

def _live_vm_count() -> int: ...
