# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, version 0.2.2.0

**Goal.** The operator asked for the version to match the Keleusma release the binding
bridges.

**Done.** The version is 0.2.2.0. The first three parts are the Keleusma release, and
the fourth counts binding releases against it. Cargo admits only three parts, so
`pyproject.toml` now holds the authoritative version and `Cargo.toml` carries 0.2.2.
`keleusma.__version__` is read from the installed distribution rather than from Cargo.
Tests check that the version extends `KELEUSMA_VERSION` and that Cargo carries its
first three parts. The migration plan now includes the bump to 0.2.3.0. The roadmap
milestone keeps its name, version 0.1.0, as a historical label.

**Still open, and why.**
- **Blocked on Keleusma.** Composite private seeding, type-generic and const-generic
  enum inputs, and `rodata`, requested from `v0.2.3`.
- **The arena margin** stays measured, not proven, until Keleusma's sizing is fixed.
- **Publishing** to PyPI needs the operator's confirmation.

**Verification.** 402 Python tests and 49 Rust tests pass locally after a rebuild,
which reported version 0.2.2.0, with pyright, ruff, rustfmt and clippy clean. The wheel
file names are verified only by the wheels workflow in CI.
