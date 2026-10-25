"""Recoverable native errors and recovery after a runtime error, the book's chapter 34.

A native reports an error the script may handle by raising `keleusma.NativeError`. The
script's `ok(v) => ..., error(code) => ...` handler then decides the result. Unhandled,
or for any other exception, the VM fails, and `vm.reset()` makes it usable again.
"""

import pickle

import pytest

import keleusma

LOOKUP = keleusma.compile(
    "use host::lookup(Word) -> Word\n"
    "fn main(id: Word) -> Word "
    "{ host::lookup(id) { ok(v) => v, error(code) => 0 - code } }"
)
UNHANDLED = keleusma.compile(
    "use host::lookup(Word) -> Word\nfn main(id: Word) -> Word { host::lookup(id) }"
)


def _lookup(id: int) -> int:
    if id < 0:
        raise keleusma.NativeError(-id, "no such row")
    return id * 10


def test_a_handled_native_error_reaches_the_script_as_its_code() -> None:
    vm = keleusma.VM(LOOKUP, natives={"host::lookup": _lookup})
    assert vm.call(4) == keleusma.Finished(40)
    assert vm.call(-7) == keleusma.Finished(-7)
    assert not vm.failed


def test_an_unhandled_native_error_is_reraised_and_fails_the_vm() -> None:
    raised = keleusma.NativeError(3, "no such row")

    def lookup(id: int) -> int:
        raise raised

    vm = keleusma.VM(UNHANDLED, natives={"host::lookup": lookup})
    with pytest.raises(keleusma.NativeError) as info:
        vm.call(1)
    assert info.value is raised
    assert vm.failed


def test_any_other_exception_fails_the_vm_even_under_a_handler() -> None:
    def lookup(id: int) -> int:
        raise KeyError(id)

    vm = keleusma.VM(LOOKUP, natives={"host::lookup": lookup})
    with pytest.raises(KeyError):
        vm.call(1)
    assert vm.failed


def test_a_code_outside_the_word_range_is_not_handled() -> None:
    # Keleusma receives the code as a Word, so a code that does not fit cannot reach
    # the handler, and the VM fails instead of wrapping it.
    def lookup(id: int) -> int:
        raise keleusma.NativeError(2**63)

    vm = keleusma.VM(LOOKUP, natives={"host::lookup": lookup})
    with pytest.raises(keleusma.NativeError):
        vm.call(1)
    assert vm.failed


@pytest.mark.parametrize("code", [True, 1.0, "1"])
def test_a_native_error_needs_an_int_code(code: object) -> None:
    with pytest.raises(TypeError, match="must be an int"):
        keleusma.NativeError(code)  # pyright: ignore[reportArgumentType]


def test_a_native_error_keeps_its_code_and_message() -> None:
    error = keleusma.NativeError(5, "busy")
    assert (error.code, error.message, str(error)) == (5, "busy", "busy")
    assert str(keleusma.NativeError(5)) == "native error code 5"
    copy = pickle.loads(pickle.dumps(error))  # noqa: S301, the bytes are made here.
    assert (copy.code, copy.message) == (5, "busy")


def test_reset_recovers_a_failed_vm() -> None:
    vm = keleusma.VM(UNHANDLED, natives={"host::lookup": _lookup})
    with pytest.raises(keleusma.NativeError):
        vm.call(-1)
    with pytest.raises(keleusma.VMError, match="failed"):
        vm.call(1)
    vm.reset()
    assert not vm.failed
    assert vm.call(2) == keleusma.Finished(20)


def test_reset_keeps_the_data_segment() -> None:
    module = keleusma.compile(
        "data st { n: Word }\n"
        "loop main(e: Word) -> Word { st.n = st.n + 1; let r = yield 10 / e; r }"
    )
    vm = keleusma.VM(module)
    assert vm.call(5) == keleusma.Yielded(2)
    vm.resume(0)
    with pytest.raises(keleusma.VMError, match="DivisionByZero"):
        vm.resume(0)
    before = vm.shared_data
    vm.reset()
    assert vm.shared_data == before
    assert vm.call(1) == keleusma.Yielded(10)


def test_reset_abandons_a_suspended_stream() -> None:
    module = keleusma.compile("loop main(t: Word) -> Word { let n = yield t; n }")
    vm = keleusma.VM(module)
    assert vm.call(1) == keleusma.Yielded(1)
    vm.reset()
    with pytest.raises(keleusma.VMError):
        vm.resume(0)
    assert vm.call(9) == keleusma.Yielded(9)


def test_a_handled_error_does_not_mask_a_later_runtime_error() -> None:
    module = keleusma.compile(
        "use host::lookup(Word) -> Word\n"
        "fn main(id: Word) -> Word "
        "{ let v = host::lookup(id) { ok(v) => v, error(code) => 0 }; 10 / v }"
    )
    vm = keleusma.VM(module, natives={"host::lookup": _lookup})
    with pytest.raises(keleusma.VMError, match="DivisionByZero"):
        vm.call(-1)


@pytest.mark.parametrize("error", [KeyboardInterrupt, ValueError])
def test_a_handler_does_not_catch_other_exceptions(
    error: type[BaseException],
) -> None:
    def lookup(id: int) -> int:
        raise error

    vm = keleusma.VM(LOOKUP, natives={"host::lookup": lookup})
    with pytest.raises(error):
        vm.call(1)
    assert vm.failed


def test_a_mistyped_native_result_is_not_handled_as_a_code() -> None:
    def lookup(id: int) -> str:
        return "text"

    vm = keleusma.VM(LOOKUP, natives={"host::lookup": lookup})
    with pytest.raises(TypeError):
        vm.call(1)
    assert vm.failed
