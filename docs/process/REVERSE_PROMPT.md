# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, issues from other lines and tasks 1 to 9

**Goal.** Attend to issues raised by other lines, complete all tasks from 1 to 9,
skipping those already done, then commit, push and merge all work.

**Issues from other lines.** One message was waiting, `pydromon-20261025-data-segment-request`,
in which `pydromon`, on the operator's instruction, asked for shared data support and an
answer of accepted, declined or deferred.

- **Accepted and implemented, as roadmap task 11.** `vm.call`, `vm.resume` and
  `vm.stream` take `shared=`, a writable byte buffer of `vm.shared_data_bytes` bytes,
  lent to Keleusma as the module's shared `data` block. The buffer is copied in and
  copied back after a successful call, which needs no `unsafe` code and keeps a Python
  native from writing to memory Keleusma is mutating. A failed call leaves it unchanged.
- **Corrected premise.** The request said private data is reset to `()`. It starts as
  `()`, but it persists across iterations once assigned, which was verified, so a
  script can already keep state inside the VM by initialising a field on its first
  iteration. The reply says so.
- **Not done.** A typed view of the shared block, because Keleusma's layout table
  expands array elements, so names and entries do not map one to one.

**Already done.** Tasks 1 to 8. Only task 9 remained.

**Done, task 9.** The README's new section "What is guaranteed, and what is not" states
that Keleusma's bounds cover the script and the bounds declared for each native, never
Python, that native bounds are promises rather than limits, what units the figures use,
and which limits are empirical in this release. A test pins those claims and runs the
README's usage example.

**Verification.** 210 Python tests and 27 Rust tests passed locally. A mutation that
skipped copying the shared buffer back made seven tests fail. `ruff`, `pyright`,
`cargo fmt` and `cargo clippy` were clean.

**Next.** All of the 0.1.0 milestone's tasks are done. A PyPI release needs the
operator's confirmation in session.
