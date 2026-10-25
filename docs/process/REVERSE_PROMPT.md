# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, the open items the binding could close, task 14

**Goal.** The operator's goal to address all known issues was judged unmet while open
items remained. This task closed those the binding itself could close.

**Closed.**
- **External natives.** `use external` natives now run. A pool of trampolines bridges
  Keleusma's function-pointer API to Python callables, with tests covering text,
  composites, native errors, nested VMs, swaps and the worst-case figure.
- **Yield guard.** The guard now refuses a text-bearing yield whose type Keleusma did
  not record. It admits a resumed value of the declared type, and refused no program in
  the corpus that Keleusma runs correctly.
- **One call per frame.** `vm.step` advances a stream one iteration per call, which
  resolves task 8's two-resumes deviation.
- **Arena use.** `vm.arena_peak_bytes` makes the empirical margin observable. The
  crate's scripts peak at most at 23 percent of their arena.

**Still open, and why.**
- **Blocked on Keleusma.** Private value access, composite private seeding,
  type-generic and const-generic enum inputs, and `rodata` all need Keleusma APIs, and
  have been requested.
- **The arena margin** stays measured, not proven, until Keleusma's sizing is fixed.
- **Publishing** to PyPI needs the operator's confirmation.

**Verification.** 394 Python tests and 47 Rust tests pass locally, with pyright, ruff,
rustfmt and clippy clean. Mutations of the guard's two new rules and of the
trampoline frame stack were each caught.
