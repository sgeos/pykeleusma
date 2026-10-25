# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, continuous integration and task 2

**Goal.** Set up remote continuous integration, complete task 1 if not yet complete,
complete task 2, and commit, push and merge all work.

**Task 1.** Already complete and merged on 2026-10-24. Its arena sizing defect belongs to
task 3 under the approved roadmap, so nothing was added to task 1.

**Continuous integration, task 10.** `.github/workflows/ci.yml` runs every local check on
Linux and macOS with Python 3.12 and 3.15, and the Rust tests under Miri with Tree
Borrows. Actions are pinned to commit hashes, using releases at least two weeks old.
The workflow has read-only permissions and no secrets.

**Task 2.** `keleusma.compile(source)` returns an immutable `Module` and releases the GIL
while compiling. Errors are Python classes in `keleusma._errors`, following
`json.JSONDecodeError`. `SourceError` derives from `KeleusmaError` and `ValueError`, and
carries `msg`, `lineno` and `colno`. The column counts characters, converted from
Keleusma's byte column. A line of 0 from Keleusma becomes `None`.

**Verification.** Locally, 26 Python tests on Python 3.12 and 3.15.1, 8 Rust tests,
`ruff`, `pyright` in strict mode, `cargo fmt` and `cargo clippy` were clean. The remote
result is in the pull request.

**Not covered.** No test shows that the GIL is released during compilation. The memory
test's behaviour on Linux is first checked by this CI run.

**Next.** Task 3, `keleusma.VM(module)` with arena sizing that survives `Reset`.
