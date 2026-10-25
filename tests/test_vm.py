"""Roadmap tasks 1 and 3. `keleusma.VM` verifies a module and owns its arena.

Task 1's tests, that a VM and its arena are freed together, now drive `keleusma.VM`.
Task 3's tests cover verification and the arena budget. That a stream survives 10,000
iterations in an automatically sized arena is tested in Rust, in `src/owned_vm.rs`,
because driving a stream from Python needs `call` and `resume`, which arrive with task
5. Those Rust tests also run under Miri, except the two long runs.
"""

# The live counter is private on purpose, because it is not public interface, and
# these tests are its only intended users.
# pyright: reportPrivateUsage=false

import sys
import threading

import pytest

import keleusma
from keleusma._keleusma import _live_vm_count

ANSWER = keleusma.compile("fn main() -> Word { 40 + 2 }")


def test_a_vm_constructs_and_runs() -> None:
    vm = keleusma.VM(ANSWER)
    assert vm.call() == keleusma.Finished(42)
    assert "arena" in repr(vm)


def test_one_module_builds_many_vms() -> None:
    # The VM takes a copy of the module, so the Python module stays usable.
    first = keleusma.VM(ANSWER)
    second = keleusma.VM(ANSWER)
    assert first.call() == second.call() == keleusma.Finished(42)


def test_the_arena_includes_the_argument_budget() -> None:
    # Both sides are derived. The difference between two VMs of the same module is
    # exactly the difference between their budgets.
    small = keleusma.VM(ANSWER, argument_bytes=0)
    large = keleusma.VM(ANSWER, argument_bytes=1000)
    assert small.argument_bytes == 0
    assert large.argument_bytes == 1000
    assert large.arena_capacity - small.arena_capacity == 1000
    default = keleusma.VM(ANSWER)
    assert default.argument_bytes == keleusma.DEFAULT_ARGUMENT_BYTES
    budget = keleusma.DEFAULT_ARGUMENT_BYTES
    assert default.arena_capacity == small.arena_capacity + budget


def test_the_argument_budget_is_keyword_only() -> None:
    with pytest.raises(TypeError):
        keleusma.VM(ANSWER, 1000)  # pyright: ignore[reportCallIssue]


@pytest.mark.parametrize("budget", [-1, 2**64])
def test_an_argument_budget_outside_the_platform_range_is_refused(budget: int) -> None:
    with pytest.raises(OverflowError):
        keleusma.VM(ANSWER, argument_bytes=budget)


def test_an_arena_too_large_to_allocate_raises_memory_error() -> None:
    # 2**63 bytes exceeds the largest allocation the platform's allocator accepts, so
    # the arena constructor refuses it without touching memory. This checks that the
    # binding reports MemoryError rather than aborting the process.
    with pytest.raises(MemoryError):
        keleusma.VM(ANSWER, argument_bytes=2**63)


def test_a_non_module_is_a_type_error() -> None:
    with pytest.raises(TypeError):
        keleusma.VM("fn main() -> Word { 1 }")  # pyright: ignore[reportArgumentType]


def test_a_module_the_verifier_rejects_raises_and_names_the_failure() -> None:
    before = _live_vm_count()
    recursive = keleusma.compile(
        "fn f(n: Word) -> Word { f(n) }\nfn main() -> Word { f(1) }"
    )
    with pytest.raises(keleusma.VerifyError, match="recursive call") as caught:
        keleusma.VM(recursive)
    assert "f:" in str(caught.value)  # The verifier names the offending chunk.
    assert isinstance(caught.value, keleusma.KeleusmaError)
    assert isinstance(caught.value, ValueError)
    assert _live_vm_count() == before


def test_a_module_without_an_entry_point_is_refused() -> None:
    with pytest.raises(keleusma.VerifyError, match="no entry point"):
        keleusma.VM(keleusma.compile(""))


def test_a_vm_with_private_data_runs_and_drops() -> None:
    # Private data lives in the arena's persistent region, which the VM's own
    # destructor walks. The arena must outlive the VM for that walk to be sound.
    module = keleusma.compile(
        "struct Point { x: Word, y: Word }\n"
        "private data d { p: Point }\n"
        "fn main() -> Word { d.p = Point { x: 3, y: 4 }; d.p.x + d.p.y }"
    )
    before = _live_vm_count()
    vm = keleusma.VM(module)
    assert vm.call() == keleusma.Finished(7)
    del vm
    assert _live_vm_count() == before


def test_dropping_a_vm_frees_it() -> None:
    before = _live_vm_count()
    vms = [keleusma.VM(ANSWER) for _ in range(100)]
    assert _live_vm_count() == before + 100
    del vms
    assert _live_vm_count() == before


def _peak_rss() -> int:
    # The unit is bytes on macOS and Windows and kibibytes on Linux. Only ratios of two
    # readings are compared, so the unit cancels. Windows has no `resource` module, so
    # its peak working set is read through the process status API.
    if sys.platform == "win32":
        import ctypes
        from ctypes import wintypes

        class Counters(ctypes.Structure):
            _fields_ = [  # noqa: RUF012, the layout ctypes requires.
                ("cb", wintypes.DWORD),
                ("PageFaultCount", wintypes.DWORD),
                ("PeakWorkingSetSize", ctypes.c_size_t),
                ("WorkingSetSize", ctypes.c_size_t),
                ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                ("PagefileUsage", ctypes.c_size_t),
                ("PeakPagefileUsage", ctypes.c_size_t),
            ]

        counters = Counters()
        counters.cb = ctypes.sizeof(Counters)
        process = ctypes.windll.kernel32.GetCurrentProcess()
        ok = ctypes.windll.psapi.GetProcessMemoryInfo(
            process, ctypes.byref(counters), counters.cb
        )
        assert ok, "GetProcessMemoryInfo failed"
        return int(counters.PeakWorkingSetSize)
    import resource

    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss


def test_memory_does_not_grow_with_the_number_of_vms_dropped() -> None:
    # Task 1's acceptance test. Peak RSS is coarse and only ever rises, so the test
    # uses a control. It first measures the growth while creating, calling and
    # dropping many VMs, then the growth while keeping as many alive. If dropping
    # leaked, the two would be comparable. The dropped case grew by a few percent of
    # the kept case on macOS and Linux, so a bound of one quarter leaves a wide margin
    # while still failing on a leak of whole VMs. A zero argument budget keeps the
    # control's 20,000 live VMs small.
    count = 20_000
    for _ in range(1_000):  # Warm the allocator so its first growth is not counted.
        keleusma.VM(ANSWER, argument_bytes=0).call()

    start = _peak_rss()
    for _ in range(count):
        vm = keleusma.VM(ANSWER, argument_bytes=0)
        vm.call()
        del vm
    dropped_growth = _peak_rss() - start

    start = _peak_rss()
    kept = [keleusma.VM(ANSWER, argument_bytes=0) for _ in range(count)]
    kept_growth = _peak_rss() - start
    assert _live_vm_count() >= count
    del kept

    # The control must itself show growth, or the measurement cannot see a leak.
    assert kept_growth > 0
    assert dropped_growth * 4 < kept_growth


def test_a_runtime_fault_raises_vm_error_and_leaves_the_vm_droppable() -> None:
    before = _live_vm_count()
    vm = keleusma.VM(keleusma.compile("fn main() -> Word { 1 / 0 }"))
    with pytest.raises(keleusma.VMError, match="DivisionByZero") as caught:
        vm.call()
    assert isinstance(caught.value, RuntimeError)
    del vm
    assert _live_vm_count() == before


def test_another_thread_cannot_use_the_vm() -> None:
    # The VM is neither Send nor Sync, so the class is unsendable and PyO3 refuses
    # access from any thread but the one that created it. PyO3 refuses by panicking,
    # which reaches Python as pyo3_runtime.PanicException, a BaseException rather
    # than an Exception. This is the one panic that crosses into Python, and it is
    # PyO3's, recorded in docs/architecture/BINDING_DESIGN.md.
    vm = keleusma.VM(ANSWER)
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
    assert vm.call() == keleusma.Finished(42)


def test_a_vm_created_on_a_worker_thread_runs_there() -> None:
    # The supported threading model. A VM belongs to the thread that creates it, so
    # concurrent programs give each VM a dedicated thread and create it there. The
    # module itself may be shared, because Module is immutable.
    results: list[keleusma.State] = []
    errors: list[BaseException] = []

    def own_and_use() -> None:
        try:
            vm = keleusma.VM(ANSWER)
            results.append(vm.call())
            results.append(vm.call())
        except BaseException as error:
            errors.append(error)

    threads = [threading.Thread(target=own_and_use) for _ in range(4)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert errors == []
    assert results == [keleusma.Finished(42)] * 8


def test_private_data_starts_as_unit_not_zero() -> None:
    # Pinned Keleusma 0.2.2 behaviour, which its own source documents. A script must
    # assign a private field before reading it, or the read fails at run time.
    module = keleusma.compile(
        "private data st { n: Word }\n"
        "loop main(e: Word) -> Word { st.n = st.n + 1; let n = yield st.n; n }"
    )
    vm = keleusma.VM(module)
    with pytest.raises(keleusma.VMError, match="Unit"):
        vm.call(1)
