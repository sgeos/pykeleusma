# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, task 5

**Goal.** Complete task 5, then commit, push and merge all work.

**Done.** `vm.call(*args)` and `vm.resume(value)` replace the private test hook. They
convert values by the declared types and return frozen dataclasses, `Yielded(value)`,
`Finished(value)` or `Reset()`, which work with `==` and `match`.

**Facts observed on Keleusma 0.2.2.** A `yield main` finishes after its last yield. A
`loop main` returns `Reset` on the resume that completes an iteration, and the next
resume starts another. The VM does not check a resumed value's type, so the binding's
conversion by the single parameter's type is the only check. Keleusma documents the
state after a runtime error as undefined, so the binding marks such a VM `failed` and
refuses further use. A conversion error and `NotSuspended` leave it usable.

**Verification.** 137 Python tests and 26 Rust tests passed locally. They include a
coroutine that yields three values and finishes, 1,000 VMs on one thread each resumed
in turn, and 10,000 stream iterations with tuple arguments driven from Python. A
mutation that stopped marking failed VMs made both failure tests fail. The README
example was run and its comments hold. `ruff`, `pyright`, `cargo fmt` and `cargo clippy`
were clean.

**Not covered.** Recovering a failed VM through Keleusma's `Vm::reset_after_error` is
not offered. A failed VM must be replaced.

**Next.** Task 5b, the worst-case time figure, then task 6, `vm.stream`.
