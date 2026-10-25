"""`vm.step`, one call per stream iteration.

A Keleusma 0.2.2 iteration of a `loop main` is a yield and then a `Reset`, so driving
one iteration per frame with `call` and `resume` takes two resumes. `step` does both.
Each expected sequence is checked against the same stream driven by `call` and
`resume`, so the tests compare the two paths rather than pin values.
"""

import pytest

import keleusma

DOUBLE = keleusma.compile("loop main(e: Word) -> Word { let n = yield e * 2; n }")


def _by_hand(module: keleusma.Module, inputs: list[int]) -> list[keleusma.State]:
    vm = keleusma.VM(module)
    states: list[keleusma.State] = [vm.call(inputs[0])]
    for value in inputs[1:]:
        state = vm.resume(value)
        if isinstance(state, keleusma.Reset):
            state = vm.resume(value)
        states.append(state)
    return states


def test_each_step_yields_one_iteration() -> None:
    inputs = [1, 2, 3, 4]
    vm = keleusma.VM(DOUBLE)
    assert [vm.step(i) for i in inputs] == _by_hand(DOUBLE, inputs)


def test_step_starts_a_fresh_stream_and_one_after_a_swap() -> None:
    vm = keleusma.VM(DOUBLE)
    assert vm.step(5) == keleusma.Yielded(10)
    assert vm.resume(0) == keleusma.Reset()
    vm.replace_module(DOUBLE)
    assert vm.step(7) == keleusma.Yielded(14)


def test_a_yield_function_runs_to_its_end_and_starts_again() -> None:
    module = keleusma.compile(
        "yield main(e: Word) -> Word { let a = yield e; let b = yield a + 1; b }"
    )
    vm = keleusma.VM(module)
    assert vm.step(1) == keleusma.Yielded(1)
    assert vm.step(5) == keleusma.Yielded(6)
    assert vm.step(9) == keleusma.Finished(9)
    assert vm.step(2) == keleusma.Yielded(2)


def test_step_lends_the_shared_block_to_each_resume() -> None:
    module = keleusma.compile(
        "data st { n: Word }\n"
        "loop main(e: Word) -> Word { st.n = st.n + e; let r = yield st.n; r }"
    )
    vm = keleusma.VM(module)
    buffer = bytearray(vm.shared_data_bytes)
    assert [vm.step(1, shared=buffer) for _ in range(3)] == [
        keleusma.Yielded(1),
        keleusma.Yielded(2),
        keleusma.Yielded(3),
    ]
    assert vm.read_shared(buffer=bytes(buffer)) == {"n": 3}


def test_step_after_a_failure_is_refused_until_reset() -> None:
    module = keleusma.compile("loop main(e: Word) -> Word { let n = yield 10 / e; n }")
    vm = keleusma.VM(module)
    with pytest.raises(keleusma.VMError, match="DivisionByZero"):
        vm.step(0)
    with pytest.raises(keleusma.VMError, match="failed"):
        vm.step(1)
    vm.reset()
    assert vm.step(5) == keleusma.Yielded(2)
