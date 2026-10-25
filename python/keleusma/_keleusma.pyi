"""Type stub for the compiled extension module."""

from collections.abc import Callable, Generator, Iterable, Mapping
from typing import final

from keleusma._native import Native
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

    def __new__(
        cls,
        module: Module,
        *,
        argument_bytes: int = ...,
        natives: Mapping[str, Callable[..., object] | Native] | None = None,
    ) -> VM: ...
    @property
    def arena_capacity(self) -> int: ...
    @property
    def argument_bytes(self) -> int: ...
    @property
    def wcet_per_iteration(self) -> int | None: ...
    @property
    def failed(self) -> bool: ...
    def register(
        self,
        name: str,
        function: Callable[..., object],
        *,
        wcet: int | None = None,
        wcmu_bytes: int | None = None,
    ) -> None: ...
    def set_native_bounds(self, name: str, wcet: int, wcmu_bytes: int) -> None: ...
    def native_bounds(self, name: str) -> tuple[int, int]: ...
    def call(self, *args: object) -> Yielded | Finished | Reset: ...
    def resume(self, value: object) -> Yielded | Finished | Reset: ...
    def stream(
        self, *args: object, inputs: Iterable[object] | None = None
    ) -> Generator[Yielded | Reset, object, object]: ...

def _live_vm_count() -> int: ...
