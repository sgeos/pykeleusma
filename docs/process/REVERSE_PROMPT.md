# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, task 4

**Goal.** Complete task 4, then commit, push and merge all work.

**Done.** Values convert between Python and Keleusma by the declared types of `main`,
which `keleusma.compile` now keeps from the parsed program. Flat composites are read at
offsets from Keleusma's own `LayoutDescriptor`, which the compiler also uses. Fixed-point
values become `keleusma.Fixed`, a `fractions.Fraction` subclass that remembers its
fraction bits, so `float(value)` works and round trips are exact. Out-of-range integers
and bytes raise `OverflowError`, and wrong types raise `TypeError`.

**Found on the way, a memory-safety defect in Keleusma 0.2.2.** Its compiler and
verifier accept a yield whose value has the wrong type. When the wrong type has the same
size as the declared one, Keleusma's safe `Vm::decode` dereferences an integer as a text
pointer and the process segfaults. This reproduces in plain Rust with no binding code.
It was reported to `v0.2.3` as `pykeleusma-20261025-yield-soundness`. The binding now
re-runs Keleusma's type inference after compiling and rejects any yield whose inferred
type differs from the declared one, as a `CompileError` pointing at the value.

**Verification.** 123 Python tests and 25 Rust tests passed locally, on Python 3.12.
Round trips cover every shape in the Dromon probe, including composites the script
builds, which are the flat ones. Two mutations confirmed the tests can fail. Reading
every field at offset zero crashed the interpreter, and disabling the guard made the
three mistyped-yield tests fail. Miri under Tree Borrows passed 23 Rust tests and
skipped the two long runs by design. `ruff`, `pyright`, `cargo fmt` and `cargo clippy`
were clean.

**Not proven.** The guard compares only expressions that Keleusma records a type for. No
crash was found past it, but only a fix in Keleusma's type checker closes the gap.

**Next.** Task 5, `vm.call` and `vm.resume`, replacing the private `VM._call` hook.
