"""Roadmap task 5b. The worst-case figures a scheduler reads from Python.

`vm.wcet_per_iteration` is Keleusma's worst-case time for one stream iteration, in its
nominal cost units, and `vm.arena_capacity` is the arena's size in bytes. The tests
compare figures between scripts rather than pinning constants, because the absolute
numbers belong to Keleusma's cost model and layout, which may change between releases.
"""

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
