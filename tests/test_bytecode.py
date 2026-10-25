"""Precompiled modules, the book's chapter 25.

`Module.to_bytes` gives Keleusma's wire format and `keleusma.load_module` reads it back.
The bytes carry no source, so a loaded module converts values without declared types,
which admits scalars and text only.
"""

import pytest

import keleusma

DOUBLE = keleusma.compile("fn main(x: Word) -> Word { x * 2 }")


def test_a_module_round_trips_through_bytes() -> None:
    loaded = keleusma.load_module(DOUBLE.to_bytes())
    assert keleusma.VM(loaded).call(21) == keleusma.VM(DOUBLE).call(21)


def test_a_loaded_module_has_no_declared_types() -> None:
    loaded = keleusma.load_module(DOUBLE.to_bytes())
    assert DOUBLE.has_declared_types
    assert not loaded.has_declared_types
    assert not loaded.is_signed


def test_the_bytes_are_deterministic() -> None:
    assert DOUBLE.to_bytes() == DOUBLE.to_bytes()
    assert keleusma.load_module(DOUBLE.to_bytes()).to_bytes() == DOUBLE.to_bytes()


def test_scalars_and_text_convert_without_declared_types() -> None:
    module = keleusma.compile(
        "fn main(x: Word, f: Float, b: bool, t: Text) -> Text { t }"
    )
    vm = keleusma.VM(keleusma.load_module(module.to_bytes()))
    assert vm.call(1, 2.5, True, "text") == keleusma.Finished("text")


def test_a_stream_runs_from_bytes() -> None:
    module = keleusma.compile("loop main(t: Word) -> Word { let n = yield t; n }")
    vm = keleusma.VM(keleusma.load_module(module.to_bytes()))
    assert vm.call(3) == keleusma.Yielded(3)
    assert vm.resume(4) == keleusma.Reset()
    assert vm.resume(5) == keleusma.Yielded(5)


def test_a_native_of_a_loaded_module_receives_scalars() -> None:
    module = keleusma.compile(
        "use host::f(Word) -> Word\nfn main(x: Word) -> Word { host::f(x) }"
    )

    def f(x: int) -> int:
        return x + 1

    vm = keleusma.VM(keleusma.load_module(module.to_bytes()), natives={"host::f": f})
    assert vm.call(1) == keleusma.Finished(2)


def test_a_composite_result_needs_declared_types() -> None:
    module = keleusma.compile("fn main(x: Word) -> (Word, Word) { (x, x) }")
    vm = keleusma.VM(keleusma.load_module(module.to_bytes()))
    with pytest.raises(TypeError, match="needs a declared type"):
        vm.call(1)


def test_a_composite_argument_needs_declared_types() -> None:
    module = keleusma.compile("fn main(p: (Word, Word)) -> Word { p.0 }")
    vm = keleusma.VM(keleusma.load_module(module.to_bytes()))
    with pytest.raises(TypeError, match="needs a declared type"):
        vm.call((1, 2))


@pytest.mark.parametrize("args", [(), (1, 2), (True,), ("a",), (1.5,)])
def test_keleusma_checks_untyped_arguments_and_the_vm_fails(
    args: tuple[object, ...],
) -> None:
    # Without declared types the binding cannot refuse before running, so Keleusma's
    # own entry check refuses, as a runtime error that fails the VM.
    vm = keleusma.VM(keleusma.load_module(DOUBLE.to_bytes()))
    with pytest.raises(keleusma.VMError, match="TypeError"):
        vm.call(*args)
    assert vm.failed
    vm.reset()
    assert vm.call(2) == keleusma.Finished(4)


def _corrupt(data: bytes, index: int) -> bytes:
    return data[:index] + bytes([data[index] ^ 1]) + data[index + 1 :]


@pytest.mark.parametrize(
    "data",
    [
        b"",
        b"not a module",
        DOUBLE.to_bytes()[:-1],
        _corrupt(DOUBLE.to_bytes(), 20),
    ],
    ids=["empty", "text", "truncated", "flipped-bit"],
)
def test_bad_bytes_raise_load_error(data: bytes) -> None:
    with pytest.raises(keleusma.LoadError):
        keleusma.load_module(data)


def test_load_error_is_a_value_error() -> None:
    with pytest.raises(ValueError):
        keleusma.load_module(b"")
