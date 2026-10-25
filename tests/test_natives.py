"""Roadmap task 7. Python callables as Keleusma natives, with declared bounds.

A native's exception reaches the caller unchanged, and a declared memory bound is
either covered by the arena or refused. Keleusma trusts the declared bounds, which a
Python callable cannot honour, so the binding's guarantee stops at the script.
"""

# The live counter is private on purpose, and these tests are its intended users.
# pyright: reportPrivateUsage=false

import gc

import pytest

import keleusma
from keleusma._keleusma import _live_vm_count

DOUBLE = keleusma.compile(
    "use host::double(Word) -> Word\n"
    "loop main(e: Word) -> Word { let n = yield host::double(e); n }"
)


def _double(x: int) -> int:
    return x * 2


def _zero(_: object) -> int:
    return 0


def _not_a_word(_: int) -> str:
    return "not a word"


def _plus_100(x: int) -> int:
    return x + 100


def _minus_one(_: int) -> int:
    return -1


def _long_pair(n: int) -> tuple[str, int]:
    return ("p" * 40, n)


def test_a_native_runs_inside_the_script() -> None:
    vm = keleusma.VM(DOUBLE, natives={"host::double": _double})
    assert vm.call(21) == keleusma.Yielded(42)
    assert vm.resume(0) == keleusma.Reset()
    assert vm.resume(5) == keleusma.Yielded(10)


def test_a_native_converts_composites_by_its_declared_signature() -> None:
    module = keleusma.compile(
        "use host::tag((Text, Word)) -> (Text, Word)\n"
        "loop main(e: (Text, Word)) -> (Text, Word) { let n = yield host::tag(e); n }"
    )

    def tag(pair: tuple[str, int]) -> tuple[str, int]:
        return (pair[0].upper(), pair[1] + 1)

    vm = keleusma.VM(module, natives={"host::tag": tag})
    assert vm.call(("wake", 41)) == keleusma.Yielded(("WAKE", 42))


def test_an_undeclared_native_converts_scalars() -> None:
    module = keleusma.compile(
        "use host::shout\nfn main(t: Text) -> Text { host::shout(t) }"
    )
    vm = keleusma.VM(module, natives={"host::shout": str.upper})
    assert vm.call("hey") == keleusma.Finished("HEY")


def test_an_undeclared_native_refuses_a_composite_argument() -> None:
    module = keleusma.compile(
        "use host::look\nfn main(e: (Text, Word)) -> Word { host::look(e) }"
    )
    vm = keleusma.VM(module, natives={"host::look": _zero})
    with pytest.raises(TypeError, match="needs a declared type"):
        vm.call(("a", 1))


# Exceptions.


class _Marker(Exception):
    """An exception whose identity the test can check."""


def test_a_python_exception_reaches_the_caller_unchanged() -> None:
    # The roadmap's acceptance test. The caller receives the very object raised.
    raised = _Marker("from the native")

    def fail(x: int) -> int:
        raise raised

    vm = keleusma.VM(DOUBLE, natives={"host::double": fail})
    with pytest.raises(_Marker) as caught:
        vm.call(1)
    assert caught.value is raised
    assert vm.failed


def test_a_base_exception_also_reaches_the_caller() -> None:
    def interrupt(x: int) -> int:
        raise KeyboardInterrupt

    vm = keleusma.VM(DOUBLE, natives={"host::double": interrupt})
    with pytest.raises(KeyboardInterrupt):
        vm.call(1)


def test_a_wrong_result_type_raises_type_error() -> None:
    vm = keleusma.VM(DOUBLE, natives={"host::double": _not_a_word})
    with pytest.raises(TypeError, match="expected Word"):
        vm.call(1)
    assert vm.failed


def test_a_native_cannot_reenter_its_own_vm() -> None:
    holder: list[keleusma.VM] = []

    def reenter(x: int) -> int:
        holder[0].call(1)
        return x

    vm = keleusma.VM(DOUBLE, natives={"host::double": reenter})
    holder.append(vm)
    with pytest.raises(RuntimeError):
        vm.call(1)
    assert vm.failed


# Registration.


def test_an_unregistered_native_is_refused_before_the_vm_runs() -> None:
    vm = keleusma.VM(DOUBLE)
    with pytest.raises(keleusma.VMError, match="not registered"):
        vm.call(1)
    assert not vm.failed
    vm.register("host::double", _double)
    assert vm.call(4) == keleusma.Yielded(8)


def test_registering_again_replaces_the_callable() -> None:
    vm = keleusma.VM(DOUBLE, natives={"host::double": _double})
    vm.register("host::double", _plus_100)
    assert vm.call(1) == keleusma.Yielded(101)


def test_an_unknown_native_name_is_refused() -> None:
    with pytest.raises(KeyError, match="host::double"):
        keleusma.VM(DOUBLE, natives={"host::doubel": _double})
    vm = keleusma.VM(DOUBLE, natives={"host::double": _double})
    with pytest.raises(KeyError):
        vm.register("host::other", _double)


def test_a_native_must_be_callable() -> None:
    with pytest.raises(TypeError, match="callable"):
        keleusma.VM(DOUBLE, natives={"host::double": 3})  # pyright: ignore[reportArgumentType]
    vm = keleusma.VM(DOUBLE, natives={"host::double": _double})
    with pytest.raises(TypeError, match="callable"):
        vm.register("host::double", 3)  # pyright: ignore[reportArgumentType]


def test_an_external_native_is_refused() -> None:
    module = keleusma.compile(
        "use external io::send\nfn main(e: Word) -> Word { io::send(e) }"
    )
    with pytest.raises(keleusma.VerifyError, match="use external"):
        keleusma.VM(module, natives={"io::send": _double})


# Bounds.


def test_bounds_default_to_keleusma_values() -> None:
    vm = keleusma.VM(DOUBLE, natives={"host::double": _double})
    assert vm.native_bounds("host::double") == (
        keleusma.DEFAULT_NATIVE_WCET,
        keleusma.DEFAULT_NATIVE_WCMU_BYTES,
    )


def test_a_declared_time_bound_adds_to_the_iteration() -> None:
    default = keleusma.VM(DOUBLE, natives={"host::double": _double})
    slow = keleusma.VM(
        DOUBLE, natives={"host::double": keleusma.Native(_double, wcet=110)}
    )
    assert default.wcet_per_iteration is not None
    assert slow.wcet_per_iteration is not None
    # One call site, so the difference is the difference between the bounds.
    assert slow.wcet_per_iteration - default.wcet_per_iteration == 110 - 10


def test_a_memory_bound_declared_at_construction_is_covered() -> None:
    small = keleusma.VM(DOUBLE, natives={"host::double": _double})
    large = keleusma.VM(
        DOUBLE, natives={"host::double": keleusma.Native(_double, wcmu_bytes=65_536)}
    )
    assert large.arena_capacity - small.arena_capacity >= 65_536
    assert large.native_bounds("host::double") == (10, 65_536)
    assert large.call(2) == keleusma.Yielded(4)


def test_a_bound_the_arena_can_hold_is_accepted_later() -> None:
    vm = keleusma.VM(
        DOUBLE, natives={"host::double": keleusma.Native(_double, wcmu_bytes=1_000)}
    )
    vm.set_native_bounds("host::double", 20, 500)
    assert vm.native_bounds("host::double") == (20, 500)


def test_a_bound_the_arena_cannot_hold_is_refused_and_reverted() -> None:
    # The roadmap's other acceptance test. The bound is refused, the earlier one
    # stays, and the VM keeps working.
    vm = keleusma.VM(DOUBLE, natives={"host::double": _double})
    before = vm.wcet_per_iteration
    with pytest.raises(keleusma.VerifyError, match="does not fit"):
        vm.set_native_bounds("host::double", 999, 1 << 20)
    assert vm.native_bounds("host::double") == (10, 0)
    # The time figure reads Keleusma's own copy of the bounds, so this shows the
    # refused bounds were reverted there too, not only in the binding's record.
    assert vm.wcet_per_iteration == before
    assert vm.call(3) == keleusma.Yielded(6)


def test_a_refused_bound_on_register_keeps_the_earlier_callable() -> None:
    vm = keleusma.VM(DOUBLE, natives={"host::double": _double})
    with pytest.raises(keleusma.VerifyError):
        vm.register("host::double", _minus_one, wcmu_bytes=1 << 20)
    assert vm.call(3) == keleusma.Yielded(6)


@pytest.mark.parametrize(("wcet", "wcmu_bytes"), [(-1, 0), (0, -1), (2**32, 0)])
def test_a_bound_outside_32_bits_raises_overflow_error(
    wcet: int, wcmu_bytes: int
) -> None:
    vm = keleusma.VM(DOUBLE, natives={"host::double": _double})
    with pytest.raises(OverflowError):
        vm.set_native_bounds("host::double", wcet, wcmu_bytes)


def test_a_native_returning_text_composites_runs_many_iterations() -> None:
    # A composite result is copied into the arena on every call. Declaring its size
    # as the native's memory bound covers it even without an argument budget.
    module = keleusma.compile(
        "use host::pair(Word) -> (Text, Word)\n"
        "loop main(e: Word) -> (Text, Word) { let n = yield host::pair(e); n }"
    )
    native = keleusma.Native(_long_pair, wcmu_bytes=256)
    vm = keleusma.VM(module, argument_bytes=0, natives={"host::pair": native})
    assert vm.call(1) == keleusma.Yielded(("p" * 40, 1))
    for step in range(2, 2_000):
        state = vm.resume(step)
        assert state in (keleusma.Reset(), keleusma.Yielded(("p" * 40, step)))


# Memory.


class _Owner:
    """Holds a VM whose native is a bound method of the owner, a reference cycle."""

    def __init__(self) -> None:
        self.vm = keleusma.VM(DOUBLE, natives={"host::double": self.double})

    def double(self, x: int) -> int:
        return x * 2


def test_a_cycle_through_a_native_is_collected() -> None:
    # The VM exposes its natives to the garbage collector, so a VM whose native refers
    # back to its owner is freed once the owner is unreachable.
    gc.collect()
    before = _live_vm_count()
    owner = _Owner()
    assert owner.vm.call(4) == keleusma.Yielded(8)
    del owner
    gc.collect()
    assert _live_vm_count() == before
