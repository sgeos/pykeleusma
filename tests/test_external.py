"""Natives imported with `use external`, served by Python callables.

Keleusma attests an external native by its calls per stream iteration rather than its
cost per call, and serves one only through a plain function pointer. The binding
routes those pointers to Python callables, as `src/external.rs` explains.
"""

import pytest

import keleusma

STREAM = keleusma.compile(
    "use external host::f(Word) -> Word\n"
    "loop main(x: Word) -> Word { let r = yield host::f(x) + 1; r }"
)


def _times_ten(x: int) -> int:
    return x * 10


def test_an_external_native_runs() -> None:
    vm = keleusma.VM(
        STREAM, natives={"host::f": keleusma.Native(_times_ten, max_invocations=1)}
    )
    assert vm.call(2) == keleusma.Yielded(21)
    assert vm.step(3) == keleusma.Yielded(31)


def test_an_external_native_needs_an_attested_call_count() -> None:
    with pytest.raises(TypeError, match="max_invocations=n"):
        keleusma.VM(STREAM, natives={"host::f": _times_ten})
    with pytest.raises(TypeError, match="max_invocations=n"):
        keleusma.VM(STREAM)


def test_an_attested_count_for_a_verified_native_is_refused() -> None:
    module = keleusma.compile(
        "use host::f(Word) -> Word\nfn main(x: Word) -> Word { host::f(x) }"
    )
    with pytest.raises(TypeError, match="applies only to a `use external` native"):
        keleusma.VM(
            module, natives={"host::f": keleusma.Native(_times_ten, max_invocations=1)}
        )


def test_text_and_composite_values_cross_an_external_native() -> None:
    module = keleusma.compile(
        "use external host::g(Text, (Word, Text)) -> (Text, Word)\n"
        "fn main(s: Text) -> (Text, Word) { host::g(s, (4, s)) }"
    )

    def g(s: str, pair: tuple[int, str]) -> tuple[str, int]:
        return (s.upper() + pair[1], pair[0])

    vm = keleusma.VM(module, natives={"host::g": keleusma.Native(g, max_invocations=1)})
    assert vm.call("ab") == keleusma.Finished(("ABab", 4))


def test_an_external_native_reports_a_recoverable_error() -> None:
    module = keleusma.compile(
        "use external host::f(Word) -> Word\n"
        "fn main(x: Word) -> Word { host::f(x) { ok(v) => v, error(c) => 0 - c } }"
    )

    def fail(x: int) -> int:
        raise keleusma.NativeError(7)

    vm = keleusma.VM(
        module, natives={"host::f": keleusma.Native(fail, max_invocations=1)}
    )
    assert vm.call(1) == keleusma.Finished(-7)


def test_a_native_may_run_another_vm_with_its_own_external_natives() -> None:
    # Each run pushes its own table, so the inner VM's call reaches the inner native
    # and the outer VM's call the outer one.
    inner_module = keleusma.compile(
        "use external host::f(Word) -> Word\n"
        "fn main(x: Word) -> Word { host::f(x) + 1000 }"
    )

    def plus_one(x: int) -> int:
        return x + 1

    inner = keleusma.VM(
        inner_module, natives={"host::f": keleusma.Native(plus_one, max_invocations=1)}
    )

    def through_inner(x: int) -> int:
        state = inner.call(x)
        assert isinstance(state, keleusma.Finished)
        assert isinstance(state.value, int)
        return state.value

    outer = keleusma.VM(
        STREAM, natives={"host::f": keleusma.Native(through_inner, max_invocations=1)}
    )
    assert outer.call(5) == keleusma.Yielded(5 + 1 + 1000 + 1)


def test_the_attested_count_scales_the_worst_case() -> None:
    def figure(max_invocations: int) -> int:
        native = keleusma.Native(_times_ten, wcet=100, max_invocations=max_invocations)
        vm = keleusma.VM(STREAM, natives={"host::f": native})
        result = vm.wcet_per_iteration
        assert result is not None
        assert vm.wcet_per_iteration_with({}) == result
        return result

    # Each attested call adds the native's declared time once.
    assert figure(3) - figure(1) == 2 * 100


def test_a_swap_keeps_an_external_native_and_refuses_a_reclassification() -> None:
    vm = keleusma.VM(
        STREAM, natives={"host::f": keleusma.Native(_times_ten, max_invocations=1)}
    )
    vm.call(1)
    vm.resume(0)
    vm.replace_module(STREAM)
    assert vm.call(4) == keleusma.Yielded(41)
    vm.resume(0)
    verified = keleusma.compile(
        "use host::f(Word) -> Word\n"
        "loop main(x: Word) -> Word { let r = yield host::f(x); r }"
    )
    with pytest.raises(keleusma.VerifyError, match="classification"):
        vm.replace_module(verified)
