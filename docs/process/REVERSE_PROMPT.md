# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, known issues, technical debt and readiness for 0.2.3, task 13

**Goal.** Address all known issues, pay off known technical debt, prepare the
architecture for Keleusma 0.2.3, then commit, push and merge.

**Resolved.** Opaque host values and `Multiword<N, F>` big numbers now convert.
Typed shared data is available through `read_shared` and `write_shared`. A swap can
seed scalar private fields, and an encrypted module can now be swapped in. Const-generic
structs convert, and custom cost tables apply through `wcet_per_iteration_with`. The
book survey is complete, Windows is tested, wheels are built and the version is 0.1.0.

**Defects found and fixed.**
- **Fuzzing crash.** A crafted module crashed the process. The binding sized its
  arena before Keleusma verified it, and Keleusma's footprint analysis then overflowed
  the stack. The binding now verifies first, and 300,000 further cases found no crash.
- **Stale closures.** A host native kept a stale declaration after a swap.
- **Windows memory test.** The memory test could not run on Windows.

**Corrected claims.**
- **Cost models.** Keleusma 0.2.2 does have a cost model API. Only the nominal table
  ships.
- **Swap resource check.** Keleusma's swap does check resources, but more loosely than
  construction.

**Migration.** `docs/architecture/MIGRATION_V0_2_3.md`, `src/compat.rs`, the canaries
and the `KELEUSMA-0.2.2` markers. The plan rests on a read-only survey, not on a build,
so each item needs confirming against the published release.

**Blocked on Keleusma, requested.** These are `use external` natives, private value
access, composite private seeding, type-generic instances, const-generic enum inputs
and a `rodata` accessor.

**Verification.** 379 Python tests and 41 Rust tests pass locally, with pyright, ruff,
rustfmt and clippy clean.
