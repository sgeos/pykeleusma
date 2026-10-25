"""A module's shared `data` block, owned by the VM in Rust and synced with Python.

Requested by the `pydromon` line, on the operator's instruction, so that a stream
processor can keep state in its module's declared data block. The VM owns the block for
its whole life. A call given `shared=` syncs the block from that Python buffer, runs,
and flushes the block back, as `src/vm.rs` explains.
Layouts are checked with `struct`, which derives the expected sizes and offsets from
the packed little-endian layout Keleusma uses, rather than from pinned numbers.
"""

import struct

import numpy as np
import pytest

import keleusma

COUNTER = keleusma.compile(
    "data st { n: Word, ticks: Word }\n"
    "loop main(e: Word) -> Word "
    "{ st.n = st.n + e; st.ticks = st.ticks + 1; let r = yield st.n; r }"
)
TWO_WORDS = "<qq"


def _buffer(n: int = 0, ticks: int = 0) -> bytearray:
    buffer = bytearray(struct.calcsize(TWO_WORDS))
    struct.pack_into(TWO_WORDS, buffer, 0, n, ticks)
    return buffer


def test_the_block_size_matches_its_declared_fields() -> None:
    assert keleusma.VM(COUNTER).shared_data_bytes == struct.calcsize(TWO_WORDS)
    plain = keleusma.compile("fn main() -> Word { 1 }")
    assert keleusma.VM(plain).shared_data_bytes == 0


def test_the_script_reads_and_writes_the_host_buffer() -> None:
    vm = keleusma.VM(COUNTER)
    buffer = _buffer(n=100)
    assert vm.call(5, shared=buffer) == keleusma.Yielded(105)
    assert struct.unpack(TWO_WORDS, buffer) == (105, 1)


def test_the_host_can_change_the_block_between_steps() -> None:
    vm = keleusma.VM(COUNTER)
    buffer = _buffer()
    vm.call(5, shared=buffer)
    assert vm.resume(0, shared=buffer) == keleusma.Reset()
    struct.pack_into("<q", buffer, 0, 1_000)
    assert vm.resume(7, shared=buffer) == keleusma.Yielded(1_007)
    assert struct.unpack(TWO_WORDS, buffer) == (1_007, 2)


def test_scalar_fields_are_packed_little_endian_in_declaration_order() -> None:
    module = keleusma.compile(
        "data st { f: Float, b: bool, x: Byte, w: Word }\n"
        "fn main() -> Word { st.f = 2.5; st.b = true; st.x = 7Byte; st.w = 0 - 3; 0 }"
    )
    layout = "<d?Bq"  # Float, bool, Byte as one unsigned byte, then Word.
    vm = keleusma.VM(module)
    assert vm.shared_data_bytes == struct.calcsize(layout)
    buffer = bytearray(vm.shared_data_bytes)
    vm.call(shared=buffer)
    assert struct.unpack(layout, buffer) == (2.5, True, 7, -3)


def test_a_stream_lends_the_same_buffer_to_every_step() -> None:
    buffer = _buffer()
    states = list(keleusma.VM(COUNTER).stream(1, inputs=[0, 2, 0, 3], shared=buffer))
    assert states[-1] == keleusma.Yielded(6)
    assert struct.unpack(TWO_WORDS, buffer) == (6, 3)


def test_a_memoryview_of_a_bytearray_works() -> None:
    vm = keleusma.VM(COUNTER)
    backing = bytearray(vm.shared_data_bytes)
    assert vm.call(4, shared=memoryview(backing)) == keleusma.Yielded(4)
    assert struct.unpack(TWO_WORDS, backing) == (4, 1)


def test_a_numpy_byte_array_works() -> None:
    vm = keleusma.VM(COUNTER)
    array = np.zeros(vm.shared_data_bytes, dtype=np.uint8)
    assert vm.call(4, shared=array) == keleusma.Yielded(4)
    assert struct.unpack(TWO_WORDS, array.tobytes()) == (4, 1)


# Refusals, which leave the VM usable because nothing ran.


def test_without_a_buffer_the_vm_keeps_the_block_itself() -> None:
    # The VM owns the block in Rust, zeroed at construction, so state persists across
    # calls that pass no Python buffer.
    vm = keleusma.VM(COUNTER)
    assert vm.shared_data == bytes(vm.shared_data_bytes)
    assert vm.call(5) == keleusma.Yielded(5)
    assert vm.resume(0) == keleusma.Reset()
    assert vm.resume(7) == keleusma.Yielded(12)
    assert struct.unpack(TWO_WORDS, vm.shared_data) == (12, 2)


def test_a_buffer_syncs_into_the_vm_and_is_flushed_back() -> None:
    vm = keleusma.VM(COUNTER)
    vm.call(5)  # The VM's own block now holds (5, 1).
    buffer = _buffer(n=100, ticks=10)
    vm.resume(0, shared=buffer)  # Synced from the buffer, so the VM forgets (5, 1).
    assert vm.resume(1, shared=buffer) == keleusma.Yielded(101)
    assert struct.unpack(TWO_WORDS, buffer) == (101, 11)
    assert vm.shared_data == bytes(buffer)


def test_a_buffer_for_a_module_without_a_block_is_refused() -> None:
    vm = keleusma.VM(keleusma.compile("fn main() -> Word { 1 }"))
    with pytest.raises(TypeError, match="must be omitted"):
        vm.call(shared=bytearray(8))


@pytest.mark.parametrize(
    ("buffer", "error", "message"),
    [
        (bytearray(8), ValueError, "has 8 bytes"),
        (bytes(16), TypeError, "writable"),
        ("sixteen chars!!!", TypeError, "buffer protocol"),
        (memoryview(bytearray(32))[::2], ValueError, "contiguous"),
    ],
)
def test_an_unsuitable_buffer_is_refused(
    buffer: object, error: type[Exception], message: str
) -> None:
    vm = keleusma.VM(COUNTER)
    with pytest.raises(error, match=message):
        vm.call(1, shared=buffer)  # pyright: ignore[reportArgumentType]
    assert not vm.failed


# What the host sees while and after the script runs.


def test_a_failed_call_leaves_the_buffer_unchanged() -> None:
    module = keleusma.compile(
        "data st { n: Word }\n"
        "loop main(e: Word) -> Word { st.n = 99; let r = yield 10 / e; r }"
    )
    vm = keleusma.VM(module)
    buffer = bytearray(struct.pack("<q", 5))
    with pytest.raises(keleusma.VMError, match="DivisionByZero"):
        vm.call(0, shared=buffer)
    assert struct.unpack("<q", buffer) == (5,)


def test_a_native_sees_the_buffer_as_it_was_before_the_call() -> None:
    module = keleusma.compile(
        "use host::peek(Word) -> Word\n"
        "data st { n: Word }\n"
        "fn main(e: Word) -> Word { st.n = e; host::peek(e) }"
    )
    buffer = bytearray(struct.pack("<q", 1))
    seen: list[int] = []

    def peek(e: int) -> int:
        seen.append(struct.unpack("<q", buffer)[0])
        return e

    vm = keleusma.VM(module, natives={"host::peek": peek})
    assert vm.call(42, shared=buffer) == keleusma.Finished(42)
    assert seen == [1]  # The script's write was not yet visible to the native.
    assert struct.unpack("<q", buffer) == (42,)  # It is after the call.


def test_the_flush_overwrites_what_python_wrote_during_the_call() -> None:
    # The VM never lends Python's memory to Keleusma. A native that writes to the
    # Python buffer during the call affects neither the run nor the result, and the
    # flush replaces its write with the VM's block. That is the Python code's concern.
    module = keleusma.compile(
        "use host::scribble(Word) -> Word\n"
        "data st { n: Word }\n"
        "fn main(e: Word) -> Word { st.n = e; host::scribble(st.n) }"
    )
    buffer = bytearray(struct.pack("<q", 1))

    def scribble(e: int) -> int:
        struct.pack_into("<q", buffer, 0, -999)
        return e

    vm = keleusma.VM(module, natives={"host::scribble": scribble})
    assert vm.call(42, shared=buffer) == keleusma.Finished(42)
    assert struct.unpack("<q", buffer) == (42,)
