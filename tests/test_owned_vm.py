"""Roadmap task 1. A VM and the arena it borrows are owned together and freed together.

These tests drive the temporary `_OwnedVmProbe` class, which task 3 replaces with
`keleusma.VM`. The Rust side has its own tests in `src/owned_vm.rs`, which also run
under Miri.
"""

# The probe and the live counter are private on purpose, because neither is public
# interface, and these tests are their only intended users.
# pyright: reportPrivateUsage=false

import resource
import threading

import pytest

from keleusma._keleusma import _live_vm_count, _OwnedVmProbe

ANSWER = "fn main() -> Word { 40 + 2 }"


def test_a_probe_constructs_and_calls() -> None:
    vm = _OwnedVmProbe(ANSWER)
    assert vm.arena_capacity > 0
    assert vm.call() == ("finished", 42)


def test_a_vm_with_private_data_runs_and_drops() -> None:
    # Private data lives in the arena's persistent region, which the VM's own
    # destructor walks. The arena must outlive the VM for that walk to be sound.
    source = (
        "struct Point { x: Word, y: Word }\n"
        "private data d { p: Point }\n"
        "fn main() -> Word { d.p = Point { x: 3, y: 4 }; d.p.x + d.p.y }"
    )
    before = _live_vm_count()
    vm = _OwnedVmProbe(source)
    assert vm.call() == ("finished", 7)
    del vm
    assert _live_vm_count() == before


def test_dropping_a_probe_frees_its_vm() -> None:
    before = _live_vm_count()
    vms = [_OwnedVmProbe(ANSWER) for _ in range(100)]
    assert _live_vm_count() == before + 100
    del vms
    assert _live_vm_count() == before


def _peak_rss() -> int:
    # The unit is bytes on macOS and kibibytes on Linux. Only ratios of two readings
    # are compared, so the unit cancels.
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss


def test_memory_does_not_grow_with_the_number_of_vms_dropped() -> None:
    # The roadmap's acceptance test. Peak RSS is coarse and only ever rises, so the
    # test uses a control. It first measures the growth while creating, calling and
    # dropping many VMs, then the growth while keeping as many alive. If dropping
    # leaked, the two would be comparable. On the development machine the dropped
    # case grew by about 3 percent of the kept case, so a bound of one quarter
    # leaves a wide margin while still failing on a leak of whole VMs.
    count = 20_000
    for _ in range(1_000):  # Warm the allocator so its first growth is not counted.
        _OwnedVmProbe(ANSWER).call()

    start = _peak_rss()
    for _ in range(count):
        vm = _OwnedVmProbe(ANSWER)
        vm.call()
        del vm
    dropped_growth = _peak_rss() - start

    start = _peak_rss()
    kept = [_OwnedVmProbe(ANSWER) for _ in range(count)]
    kept_growth = _peak_rss() - start
    assert _live_vm_count() >= count
    del kept

    # The control must itself show growth, or the measurement cannot see a leak.
    assert kept_growth > 0
    assert dropped_growth * 4 < kept_growth


def test_a_module_the_verifier_rejects_is_refused_and_counts_nothing() -> None:
    before = _live_vm_count()
    recursive = "fn f(n: Word) -> Word { f(n) }\nfn main() -> Word { f(1) }"
    with pytest.raises(ValueError, match="recursive"):
        _OwnedVmProbe(recursive)
    assert _live_vm_count() == before


def test_source_that_does_not_parse_reports_its_location() -> None:
    with pytest.raises(ValueError, match=r"parse error at 1:21"):
        _OwnedVmProbe("fn main() -> Word { @ }")


def test_a_runtime_fault_raises_and_leaves_the_vm_droppable() -> None:
    before = _live_vm_count()
    vm = _OwnedVmProbe("fn main() -> Word { 1 / 0 }")
    with pytest.raises(RuntimeError, match="DivisionByZero"):
        vm.call()
    del vm
    assert _live_vm_count() == before


def test_another_thread_cannot_use_the_vm() -> None:
    # The VM is neither Send nor Sync, so the class is unsendable and PyO3 refuses
    # access from any thread but the one that created it. PyO3 refuses by panicking,
    # which reaches Python as pyo3_runtime.PanicException, a BaseException rather
    # than an Exception. This is the one panic that crosses into Python, and it is
    # PyO3's, recorded in docs/architecture/BINDING_DESIGN.md.
    vm = _OwnedVmProbe(ANSWER)
    errors: list[BaseException] = []

    def use() -> None:
        try:
            vm.call()
        except BaseException as error:
            errors.append(error)

    thread = threading.Thread(target=use)
    thread.start()
    thread.join()
    assert len(errors) == 1
    assert type(errors[0]).__name__ == "PanicException"
    assert not isinstance(errors[0], Exception)
    assert "unsendable" in str(errors[0])
    assert vm.call() == ("finished", 42)
