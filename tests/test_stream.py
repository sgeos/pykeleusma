"""Roadmap task 6. `vm.stream(...)` drives a VM through the generator protocol.

The generator yields `Yielded` and `Reset` states and returns the `Finished` value.
Iteration ends on `Finished`. A `Reset` means that a `loop main` finished one iteration
and that the next input starts the next one, as `keleusma._stream` documents.
"""

from collections.abc import Generator

import pytest

import keleusma

THREE_YIELDS = keleusma.compile(
    "yield main(e: Word) -> Word "
    "{ let a = yield e; let b = yield a + 1; let c = yield b + 1; c + 1 }"
)
DOUBLER = keleusma.compile("loop main(e: Word) -> Word { let n = yield e * 2; n }")


def _drain(
    generator: Generator[keleusma.Yielded | keleusma.Reset, object, object],
) -> tuple[list[keleusma.Yielded | keleusma.Reset], object]:
    """Collect every state and the generator's return value."""
    states: list[keleusma.Yielded | keleusma.Reset] = []
    while True:
        try:
            states.append(next(generator))
        except StopIteration as stop:
            return states, stop.value


def test_iteration_ends_on_finished_and_returns_its_value() -> None:
    # The roadmap's acceptance test, with inputs from an iterable.
    states, result = _drain(keleusma.VM(THREE_YIELDS).stream(1, inputs=[10, 20, 30]))
    assert states == [keleusma.Yielded(1), keleusma.Yielded(11), keleusma.Yielded(21)]
    assert result == 31


def test_a_for_loop_ends_on_finished() -> None:
    seen = [state for state in keleusma.VM(THREE_YIELDS).stream(1, inputs=[10, 20, 30])]
    assert seen == [keleusma.Yielded(1), keleusma.Yielded(11), keleusma.Yielded(21)]


def test_yield_from_receives_the_finished_value() -> None:
    vm = keleusma.VM(THREE_YIELDS)

    def outer() -> Generator[keleusma.Yielded | keleusma.Reset, object, object]:
        result = yield from vm.stream(1, inputs=[10, 20, 30])
        return ("done", result)

    assert _drain(outer())[1] == ("done", 31)


def test_send_supplies_each_resume_value() -> None:
    generator = keleusma.VM(THREE_YIELDS).stream(1)
    assert next(generator) == keleusma.Yielded(1)
    assert generator.send(10) == keleusma.Yielded(11)
    assert generator.send(20) == keleusma.Yielded(21)
    with pytest.raises(StopIteration) as stop:
        generator.send(30)
    assert stop.value.value == 31


def test_a_reset_marks_the_end_of_an_iteration() -> None:
    # The stated meaning. The input that completes an iteration yields Reset, and
    # the next input starts the next iteration as its argument.
    states, result = _drain(keleusma.VM(DOUBLER).stream(1, inputs=[10, 20, 0, 7]))
    assert states == [
        keleusma.Yielded(2),
        keleusma.Reset(),
        keleusma.Yielded(40),
        keleusma.Reset(),
        keleusma.Yielded(14),
    ]
    assert result is None  # A loop never finishes, so the stream ended with its inputs.


def test_a_unit_stream_runs_in_a_plain_for_loop() -> None:
    # next() sends None, which converts where the parameter is declared as ().
    vm = keleusma.VM(
        keleusma.compile("loop main(e: ()) -> Word { let n = yield 7; n }")
    )
    seen: list[keleusma.Yielded | keleusma.Reset] = []
    for state in vm.stream(None):
        seen.append(state)
        if len(seen) == 5:
            break
    assert seen == [keleusma.Yielded(7), keleusma.Reset()] * 2 + [keleusma.Yielded(7)]


def test_nothing_runs_until_the_generator_is_advanced() -> None:
    vm = keleusma.VM(
        keleusma.compile("loop main(e: Word) -> Word { let n = yield 10 / e; n }")
    )
    generator = vm.stream(0)  # Would divide by zero if it ran.
    assert not vm.failed
    with pytest.raises(keleusma.VMError, match="DivisionByZero"):
        next(generator)
    assert vm.failed


def test_leaving_early_keeps_the_vm_resumable() -> None:
    vm = keleusma.VM(DOUBLER)
    for state in vm.stream(3, inputs=[0, 5]):
        assert state == keleusma.Yielded(6)
        break
    assert vm.resume(0) == keleusma.Reset()
    assert vm.resume(9) == keleusma.Yielded(18)


def test_send_is_refused_when_inputs_are_given() -> None:
    generator = keleusma.VM(DOUBLER).stream(1, inputs=[2])
    next(generator)
    with pytest.raises(TypeError, match="takes its inputs from"):
        generator.send(5)


def test_a_wrong_input_type_ends_the_stream_with_type_error() -> None:
    generator = keleusma.VM(DOUBLER).stream(1)
    next(generator)
    with pytest.raises(TypeError):
        next(generator)  # Sends None to a Word parameter.


def test_a_plain_function_finishes_without_yielding() -> None:
    vm = keleusma.VM(keleusma.compile("fn main(a: Word) -> Word { a + 1 }"))
    assert _drain(vm.stream(41)) == ([], 42)
