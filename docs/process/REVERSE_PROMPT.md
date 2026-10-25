# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, task 3

**Goal.** Complete task 3, then commit, push and merge all work.

**Done.** `keleusma.VM(module, *, argument_bytes=4096)` replaces `_OwnedVmProbe`. It
verifies the module, raising `VerifyError` with the verifier's message, which names the
chunk and the failure. Other Keleusma errors raise `VMError`. The arena is the sum of
Keleusma's footprint, the persistent region, twice the working set as a regrowth
margin, and a host-chosen argument budget. `vm.arena_capacity` and `vm.argument_bytes`
report it.

**Why the margin.** `v0.2.3` has not answered how a host should size the arena, beyond
calling a margin a workaround. Measurements on seven scripts showed that the shortfall
after a `Reset` is one doubling of the operand stack, which the margin covers with at
least 48 bytes to spare. Composite arguments cost about twice their text length and
accumulate across the yields of one iteration, so they need a budget the host chooses.

**Verification.** 34 Python tests and 15 Rust tests passed locally, including seven
word streams and one tuple stream run for 10,000 iterations in an automatically sized
arena. A mutation that removed the margin made the word-stream tests fail. Miri under
Tree Borrows passed 13 tests and skipped the two long runs by design. `ruff`, `pyright`,
`cargo fmt` and `cargo clippy` were clean.

**Not proven.** The margin is empirical. A script that needs two regrowths would exceed
it, and none was found. The 10,000-iteration test runs in Rust, because Python cannot
drive a stream until task 5.

**Next.** Task 4, value conversion by declared type.
