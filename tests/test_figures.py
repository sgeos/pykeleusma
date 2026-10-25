"""Roadmap task 5b. The worst-case figures a scheduler reads from Python.

`vm.wcet_per_iteration` is Keleusma's worst-case time for one stream iteration, in
nominal cycles, and `vm.arena_capacity` is the arena's size in bytes. The tests
compare figures between scripts rather than pinning constants, because the absolute
numbers belong to Keleusma's cost model and layout, which may change between releases.
"""

import pytest

import keleusma

STREAM = "loop main(e: Word) -> Word {{ let n = yield {expression}; n }}"


def _wcet(source: str) -> int | None:
    return keleusma.VM(keleusma.compile(source)).wcet_per_iteration


def test_only_a_stream_has_a_per_iteration_time() -> None:
    # Keleusma reports zero for a module without a stream chunk, which the binding
    # turns into None so that it cannot be read as a free iteration.
    assert _wcet("fn main(e: Word) -> Word { e }") is None
    assert _wcet("yield main(e: Word) -> Word { let a = yield e; a }") is None
    figure = _wcet(STREAM.format(expression="e * 2"))
    assert isinstance(figure, int)
    assert figure > 0


def test_more_work_in_an_iteration_costs_more() -> None:
    simple = _wcet(STREAM.format(expression="e * 2"))
    larger = _wcet(STREAM.format(expression="((e * 2) + (e * 3)) * (e - 1)"))
    assert simple is not None
    assert larger is not None
    assert larger > simple


def test_a_called_function_counts_towards_the_iteration() -> None:
    helper = "fn f(x: Word) -> Word { x * x }\n"
    once = _wcet(helper + STREAM.format(expression="f(e)"))
    twice = _wcet(helper + STREAM.format(expression="f(e) + f(e + 1)"))
    assert once is not None
    assert twice is not None
    assert twice > once


def test_the_time_does_not_depend_on_the_arena_or_on_running() -> None:
    module = keleusma.compile(STREAM.format(expression="e * 2"))
    small = keleusma.VM(module, argument_bytes=0)
    large = keleusma.VM(module, argument_bytes=100_000)
    assert small.wcet_per_iteration == large.wcet_per_iteration
    before = small.wcet_per_iteration
    small.call(1)
    small.resume(2)
    assert small.wcet_per_iteration == before


def test_a_deeper_stack_needs_a_larger_arena() -> None:
    shallow = keleusma.VM(
        keleusma.compile(STREAM.format(expression="e")), argument_bytes=0
    )
    deep = keleusma.VM(
        keleusma.compile(
            STREAM.format(expression="((e + 1) * (e + 2)) + ((e + 3) * (e + 4))")
        ),
        argument_bytes=0,
    )
    assert deep.arena_capacity > shallow.arena_capacity


def test_the_argument_budget_adds_to_the_arena_exactly() -> None:
    module = keleusma.compile(STREAM.format(expression="e"))
    without = keleusma.VM(module, argument_bytes=0)
    with_budget = keleusma.VM(module, argument_bytes=512)
    assert with_budget.arena_capacity - without.arena_capacity == 512


# Cost models of the host's, applied by opcode name.

STEP = keleusma.compile(
    "use host::f(Word) -> Word\n"
    "loop main(x: Word) -> Word { let y = x + x * 3; let r = yield host::f(y); r }"
)


def _identity(x: int) -> int:
    return x


def _step_vm(wcet: int = 50) -> keleusma.VM:
    return keleusma.VM(STEP, natives={"host::f": keleusma.Native(_identity, wcet=wcet)})


def test_an_empty_table_gives_the_nominal_figure() -> None:
    vm = _step_vm()
    assert vm.wcet_per_iteration_with({}) == vm.wcet_per_iteration


def test_free_opcodes_leave_only_the_declared_native_bound() -> None:
    zero = dict.fromkeys(keleusma.OP_NAMES, 0)
    for wcet in (0, 50, 1_000):
        assert _step_vm(wcet).wcet_per_iteration_with(zero, text_byte_cycles=0) == wcet


def test_a_cost_grows_the_figure_once_per_execution() -> None:
    # The script runs one checked add per iteration, so raising its cost by `k` raises
    # the figure by exactly `k`. Both sides are measured, not pinned.
    vm = _step_vm()
    base = vm.wcet_per_iteration_with({"CheckedAdd": 10})
    assert base is not None
    assert vm.wcet_per_iteration_with({"CheckedAdd": 17}) == base + 7


def test_an_unknown_opcode_is_refused() -> None:
    with pytest.raises(ValueError, match="not a Keleusma opcode"):
        _step_vm().wcet_per_iteration_with({"Nope": 1})


def test_a_module_without_a_stream_has_no_figure() -> None:
    vm = keleusma.VM(keleusma.compile("fn main() -> Word { 1 }"))
    assert vm.wcet_per_iteration_with({"CheckedAdd": 5}) is None


# The arena's measured use, which shows how much of the sizing margin a host's own
# scripts need.


def test_the_peak_grows_with_use_and_stays_within_the_arena() -> None:
    module = keleusma.compile(
        "loop main(e: (Text, Word)) -> (Text, Word) { let n = yield e; n }"
    )
    vm = keleusma.VM(module)
    fresh = vm.arena_peak_bytes
    vm.call(("abc", 1))
    after_one = vm.arena_peak_bytes
    for i in range(1_000):
        vm.step(("abcdefgh", i))
    assert fresh <= after_one <= vm.arena_peak_bytes <= vm.arena_capacity
    assert after_one > fresh


def test_the_crate_scripts_leave_headroom() -> None:
    # Measured, not proven: every vendored script peaks below the arena it was sized
    # for, which is what the empirical margin is meant to ensure.
    from pathlib import Path

    scripts = sorted((Path(__file__).parent / "keleusma_scripts").glob("*.kel"))
    assert scripts
    for script in scripts:
        vm = keleusma.VM(keleusma.compile(script.read_text(encoding="utf-8")))
        vm.call()
        assert vm.arena_peak_bytes < vm.arena_capacity, script.name
