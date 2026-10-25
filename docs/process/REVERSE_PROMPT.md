# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-24, scratch directories, housekeeping and task 1

**Done.** Ignored `tmp/` and `secret/` directories following Keleusma, with
`tmp/.gitkeep` tracked. Housekeeping fixed a stale-build defect in `uv sync`, ignored
`.claude/`, and brought the status text up to date. Task 1 is done. `OwnedVm` owns the
arena and the VM with `self_cell`, and both operator decisions are recorded in the design
document. Everything is on the branch `feat/owned-vm`, which is not merged or pushed.

**Verification.** `uv run pytest` passed 11 tests and `cargo test` passed 3. `ruff
check`, `ruff format --check`, `pyright` in strict mode, `cargo fmt --check` and `cargo
clippy --all-targets -- -D warnings` were clean. The Rust tests passed under Miri with
Tree Borrows. Under the default Stacked Borrows model, Miri reported undefined behaviour
inside rkyv 0.8.18 during `Vm::new`, which was reported to `v0.2.3`. The memory test was
run five times without a failure. A deliberate leak of one VM per construction made it
fail, so it can detect the defect it guards against. Only macOS with Python 3.12.12 was
checked.

**Not verified.** That the memory test is stable on Linux, where peak memory is reported
in different units and the allocator differs. The ratio comparison is meant to be
independent of both, but it has not run there.

**Questions.**
1. May `feat/owned-vm` be pushed and merged into `main` through a pull request?
2. Is PyO3's `PanicException` for cross-thread use acceptable? The alternative was
   rejected as unsound, as the design document explains.

**Next.** Task 2, `keleusma.compile` with located exceptions.
