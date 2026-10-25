# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-24, the seed

**Verification.** `uv sync` built the extension. `uv run pytest` passed three tests.
`ruff check`, `ruff format --check`, `pyright` in strict mode, `cargo fmt --check` and
`cargo clippy --all-targets -- -D warnings` were clean. The built module reported
Keleusma 0.2.2 and bytecode version 1. Only macOS with Python 3.12.12 was checked, and no
continuous integration exists yet. The repository is public at
https://github.com/sgeos/pykeleusma.

**Questions.** The three items under "Open with the operator" in `HANDOFF.md`.

**Next.** Task 1 of the 0.1.0 milestone, owning the arena and the VM together.
