"""Roadmap task 5. `vm.call`, `vm.resume` and the state they return.

A `yield main` is a coroutine that finishes, and a `loop main` is a stream that resets
after each iteration and never finishes. The tests drive both, check which failures
leave a VM usable, and run many VMs on one thread, which Dromon's batched update loop
needs.
"""

import dataclasses

import pytest

import keleusma

THREE_YIELDS = keleusma.compile(
    "yield main(e: Word) -> Word "
    "{ let a = yield e; let b = yield a + 1; let c = yield b + 1; c + 1 }"
)
DOUBLER = keleusma.compile("loop main(e: Word) -> Word { let n = yield e * 2; n }")


def test_a_script_that_yields_three_values_runs_to_completion() -> None:
    # The roadmap's acceptance test. Each resume value is the next yield's input.
    vm = keleusma.VM(THREE_YIELDS)
    assert vm.call(1) == keleusma.Yielded(1)
    assert vm.resume(10) == keleusma.Yielded(11)
    assert vm.resume(20) == keleusma.Yielded(21)
    assert vm.resume(30) == keleusma.Finished(31)


def test_states_support_pattern_matching() -> None:
    vm = keleusma.VM(THREE_YIELDS)
    state = vm.call(5)
    seen: list[object] = []
    while True:
        match state:
            case keleusma.Yielded(value):
                seen.append(value)
                state = vm.resume(len(seen) * 100)
            case keleusma.Finished(value):
                seen.append(("finished", value))
                break
            case keleusma.Reset():
                pytest.fail("a yield main never resets")
    assert seen == [5, 101, 201, ("finished", 301)]


def test_a_stream_resets_after_each_iteration() -> None:
    # The resume that completes an iteration returns Reset, and the next resume
    # starts a new iteration with that value as its argument.
    vm = keleusma.VM(DOUBLER)
    assert vm.call(1) == keleusma.Yielded(2)
    assert vm.resume(10) == keleusma.Reset()
    assert vm.resume(20) == keleusma.Yielded(40)
    assert vm.resume(0) == keleusma.Reset()
    assert vm.resume(7) == keleusma.Yielded(14)


def test_a_plain_function_finishes_on_call() -> None:
    vm = keleusma.VM(keleusma.compile("fn main(a: Word, b: Word) -> Word { a * b }"))
    assert vm.call(6, 7) == keleusma.Finished(42)


def test_states_are_immutable_values() -> None:
    state = keleusma.Yielded(1)
    assert state == keleusma.Yielded(1)
    assert state != keleusma.Finished(1)
    assert keleusma.Reset() == keleusma.Reset()
    assert repr(state) == "Yielded(value=1)"
    with pytest.raises(dataclasses.FrozenInstanceError):
        state.value = 2  # pyright: ignore[reportAttributeAccessIssue]


def test_tuple_events_resume_a_stream() -> None:
    vm = keleusma.VM(
        keleusma.compile(
            "loop main(e: (Text, Word)) -> (Text, Word) "
            "{ let n = yield (e.0, e.1 + 1); n }"
        )
    )
    assert vm.call(("tokens", 1)) == keleusma.Yielded(("tokens", 2))
    assert vm.resume(("ignored", 0)) == keleusma.Reset()
    assert vm.resume(("wake", 41)) == keleusma.Yielded(("wake", 42))


def test_a_stream_runs_ten_thousand_iterations_from_python() -> None:
    # Task 3's acceptance test, repeated through the Python interface with tuple
    # arguments of varying length in an automatically sized arena.
    vm = keleusma.VM(
        keleusma.compile(
            "loop main(e: (Text, Text)) -> (Text, Text) "
            '{ let n = yield ("send", e.1); n }'
        )
    )
    texts = ["hello", "a longer piece of host text"]
    assert vm.call(("tokens", texts[0])) == keleusma.Yielded(("send", texts[0]))
    for step in range(1, 10_000):
        text = texts[step % 2]
        state = vm.resume(("tokens", text))
        # Odd steps complete an iteration and even steps start the next one.
        expected = keleusma.Reset() if step % 2 else keleusma.Yielded(("send", text))
        assert state == expected


def test_a_thousand_vms_on_one_thread_are_each_resumed_in_turn() -> None:
    # The roadmap's acceptance test for Dromon's batched update loop. One module
    # builds every VM, and each VM is resumed once per tick in a fixed order.
    vms = [keleusma.VM(DOUBLER, argument_bytes=0) for _ in range(1_000)]
    for index, vm in enumerate(vms):
        assert vm.call(index) == keleusma.Yielded(2 * index)
    for tick in range(4):
        for index, vm in enumerate(vms):
            state = vm.resume(index + tick)
            # Each VM alternates between completing and starting an iteration.
            if tick % 2 == 0:
                assert state == keleusma.Reset()
            else:
                assert state == keleusma.Yielded(2 * (index + tick))


# Failures, and whether the VM survives them.


def test_resume_before_call_raises_and_leaves_the_vm_usable() -> None:
    vm = keleusma.VM(DOUBLER)
    with pytest.raises(keleusma.VMError, match="NotSuspended"):
        vm.resume(1)
    assert not vm.failed
    assert vm.call(3) == keleusma.Yielded(6)


def test_resume_after_finished_raises_and_leaves_the_vm_usable() -> None:
    vm = keleusma.VM(THREE_YIELDS)
    vm.call(0)
    for value in (1, 2, 3):
        vm.resume(value)
    with pytest.raises(keleusma.VMError, match="NotSuspended"):
        vm.resume(4)
    assert not vm.failed


def test_a_plain_function_cannot_be_resumed() -> None:
    vm = keleusma.VM(keleusma.compile("fn main(e: Word) -> Word { e }"))
    vm.call(1)
    with pytest.raises(TypeError, match="cannot be resumed"):
        vm.resume(2)


def test_a_resume_value_of_the_wrong_type_raises_before_the_vm_runs() -> None:
    # Keleusma does not check a resumed value's type, so the binding's conversion is
    # the only check, and it runs before the VM is entered.
    vm = keleusma.VM(DOUBLER)
    vm.call(1)
    with pytest.raises(TypeError):
        vm.resume("not a word")
    assert not vm.failed
    assert vm.resume(5) == keleusma.Reset()


def test_a_runtime_error_marks_the_vm_failed() -> None:
    # Keleusma documents the state after a runtime error as undefined, so the
    # binding refuses further use rather than continuing in it.
    vm = keleusma.VM(
        keleusma.compile("loop main(e: Word) -> Word { let n = yield 10 / e; n }")
    )
    assert vm.call(2) == keleusma.Yielded(5)
    assert vm.resume(1) == keleusma.Reset()
    with pytest.raises(keleusma.VMError, match="DivisionByZero"):
        vm.resume(0)
    assert vm.failed
    with pytest.raises(keleusma.VMError, match="failed earlier"):
        vm.resume(1)
    with pytest.raises(keleusma.VMError, match="failed earlier"):
        vm.call(1)


def test_an_oversized_resume_argument_marks_the_vm_failed() -> None:
    vm = keleusma.VM(
        keleusma.compile("loop main(e: (Text, Text)) -> Word { let n = yield 1; n }"),
        argument_bytes=64,
    )
    assert vm.call(("a", "b")) == keleusma.Yielded(1)
    assert vm.resume(("a", "b")) == keleusma.Reset()
    with pytest.raises(keleusma.VMError, match="OutOfArena"):
        vm.resume(("tokens", "x" * 8192))
    assert vm.failed
