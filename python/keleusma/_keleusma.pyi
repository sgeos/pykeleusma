"""Type stub for the compiled extension module."""

from collections.abc import Buffer, Callable, Generator, Iterable, Mapping, Sequence
from typing import final

from keleusma._native import Native
from keleusma._state import Finished, Reset, Yielded

__version__: str
KELEUSMA_VERSION: str
BYTECODE_VERSION: int
DEFAULT_ARGUMENT_BYTES: int
OP_NAMES: tuple[str, ...]

@final
class Module:
    """A compiled Keleusma module. Immutable."""

    def to_bytes(self) -> bytes: ...
    def to_signed_bytes(
        self, signing_seed: bytes, *, recipient_key: bytes | None = None
    ) -> bytes: ...
    @property
    def is_signed(self) -> bool: ...
    @property
    def has_declared_types(self) -> bool: ...

def compile(source: str) -> Module:
    """Compile Keleusma source text into a module.

    Raises `keleusma.LexError`, `keleusma.ParseError` or `keleusma.CompileError`.
    """

def load_module(
    data: bytes,
    *,
    verifying_keys: Sequence[bytes] | None = None,
    decryption_key: bytes | None = None,
) -> Module:
    """Load a module from wire-format bytes, verifying or decrypting it as needed.

    Bytecode is code. Load unsigned bytes only from a source trusted as the script's
    own source would be, because Keleusma's decoder is not known to be robust against
    crafted input. A signature identifies the producer, it does not make bytes safe.
    """

def library_signatures(name: str) -> str: ...
def signing_public_key(seed: bytes) -> bytes: ...
def encryption_public_key(private_key: bytes) -> bytes: ...

@final
class VM:
    """A verified Keleusma VM that owns its arena. Belongs to the creating thread."""

    def __new__(
        cls,
        module: Module,
        *,
        argument_bytes: int = ...,
        natives: Mapping[str, Callable[..., object] | Native] | None = None,
        libraries: Sequence[str] | None = None,
    ) -> VM: ...
    @property
    def arena_capacity(self) -> int: ...
    @property
    def argument_bytes(self) -> int: ...
    @property
    def arena_peak_bytes(self) -> int: ...
    @property
    def wcet_per_iteration(self) -> int | None: ...
    def wcet_per_iteration_with(
        self, op_cycles: Mapping[str, int], *, text_byte_cycles: int | None = None
    ) -> int | None: ...
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
    def replace_module(
        self, module: Module, *, private: Mapping[str, object] | None = None
    ) -> None: ...
    def register_verifying_key(self, key: bytes) -> None: ...
    def reset(self) -> None: ...
    @property
    def shared_data_bytes(self) -> int: ...
    @property
    def shared_data(self) -> bytes: ...
    def read_shared(self, buffer: Buffer | None = None) -> dict[str, object]: ...
    def write_shared(
        self, values: Mapping[str, object], buffer: Buffer | None = None
    ) -> None: ...
    def call(
        self, *args: object, shared: Buffer | None = None
    ) -> Yielded | Finished | Reset: ...
    def resume(
        self, value: object, *, shared: Buffer | None = None
    ) -> Yielded | Finished | Reset: ...
    def step(
        self, value: object, *, shared: Buffer | None = None
    ) -> Yielded | Finished | Reset: ...
    def stream(
        self,
        *args: object,
        inputs: Iterable[object] | None = None,
        shared: Buffer | None = None,
    ) -> Generator[Yielded | Reset, object, object]: ...

def _live_vm_count() -> int: ...
