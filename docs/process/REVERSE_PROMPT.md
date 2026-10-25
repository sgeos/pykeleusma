# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, tasks 1 to 7

**Goal.** Complete all tasks from 1 to 7, skipping those already done, then commit,
push and merge all work.

**Already done.** Tasks 1 to 6, merged through pull requests 1 to 9. Only task 7
remained.

**Done, task 7.** Python callables serve as Keleusma natives. They are passed at
construction, `VM(module, natives={"host::f": f})` or with declared bounds through
`keleusma.Native(f, wcet=..., wcmu_bytes=...)`, or later through `vm.register`.
`vm.set_native_bounds` and `vm.native_bounds` manage the bounds.

- **A native's exception reaches the caller unchanged.** The caller of `call` or
  `resume` receives the very object the native raised, including `KeyboardInterrupt`,
  and the VM is then marked failed.
- **Declared bounds are covered or refused.** Memory bounds given at construction size
  the arena. A later bound the arena cannot hold raises `VerifyError` and is reverted
  in Keleusma as well as in the binding.
- **Cycles are collected.** The VM exposes its natives to Python's garbage collector,
  so a native bound to the object that owns its VM does not leak.
- **Missing natives** are refused before the VM runs, so the VM stays usable.

**Found.** A native's composite result is copied into the arena on every call, so a
native returning text tuples needs a declared `wcmu_bytes` or relies on the argument
budget.

**Verification.** 178 Python tests and 27 Rust tests passed locally. Three mutations,
not re-raising, not reverting and not traversing, each made tests fail. The first
version of the revert test let its mutation survive, and was strengthened to check the
time figure, which reads Keleusma's own copy of the bounds. The README's native pattern
was run. `ruff`, `pyright`, `cargo fmt` and `cargo clippy` were clean.

**Next.** Task 8, the examples.
