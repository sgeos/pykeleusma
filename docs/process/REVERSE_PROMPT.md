# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, tasks 1 to 5b

**Goal.** Complete all tasks from 1 to 5b, skipping those already done, then commit,
push and merge all work.

**Already done.** Tasks 1 to 5, merged through pull requests 1 to 7. Only task 5b
remained.

**Done, task 5b.** `vm.wcet_per_iteration` returns Keleusma's worst-case time for one
iteration of a `loop main`, in its nominal cost units. `vm.arena_capacity`, added in
task 3, gives the arena's size in bytes. The design document, the README and the
docstrings state the units and that both figures cover only the script and the declared
bounds of natives, never Python or the binding's conversions.

**Fact.** Keleusma reports a time of zero for a `fn main` or a `yield main`, because only
a stream chunk has a per-iteration bound. The binding returns `None` for those, so a
missing figure cannot be read as a free iteration.

**Correction for Dromon.** Its integration document expected pipelined cycles. The units
are Keleusma's unitless nominal cost units, which need a calibrated cost model to become
cycles or seconds.

**Verification.** 143 Python tests and 26 Rust tests passed locally. The tests compare
figures between scripts rather than pinning constants. A mutation that let Keleusma's
zero through made the `None` test fail. `ruff`, `pyright`, `cargo fmt` and `cargo clippy`
were clean.

**Next.** Task 6, `vm.stream(...)` as a generator.
