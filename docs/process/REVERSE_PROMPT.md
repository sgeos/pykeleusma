# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, tasks 1 to 6

**Goal.** Complete all tasks from 1 to 6, skipping those already done, then commit,
push and merge all work.

**Already done.** Tasks 1 to 5b, merged through pull requests 1 to 8. Only task 6
remained.

**Done, task 6.** `vm.stream(*args, inputs=None)` returns a generator that yields each
`Yielded` and `Reset` state and returns the `Finished` value, so iteration ends on
`Finished`. Resume inputs come from the `inputs` iterable or from `send`. `next()` sends
`None`, which suits streams whose parameter is `()`.

**The stated meaning of `Reset`.** A `loop main` never finishes. It reports `Reset`
after each iteration, and the next input starts the next iteration as its argument. The
generator yields `keleusma.Reset()` at that boundary.

**Verification.** 154 Python tests and 26 Rust tests passed locally. A mutation that kept
iterating past `Finished` made five tests fail. The README's streaming pattern was run.
`ruff`, `pyright`, `cargo fmt` and `cargo clippy` were clean.

**Next.** Task 7, Python natives with declared bounds.
