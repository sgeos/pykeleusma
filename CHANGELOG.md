# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `OwnedVm`, which owns a Keleusma arena and the VM that borrows it as one `self_cell`
  value, sizes the arena from the module, and raises `MemoryError` rather than aborting
  when the arena cannot be allocated. A temporary private class `_OwnedVmProbe` and a
  live-VM counter exercise it from Python until `keleusma.VM` arrives.
- Ignored project-local `tmp/` and `secret/` directories, following the Keleusma
  repository.

### Fixed

- `uv sync` now rebuilds the extension after a change to the Rust sources, through
  `cache-keys` in `pyproject.toml`. Before, it kept a stale build.

- The repository seed. A PyO3 extension module `keleusma._keleusma`, built by maturin
  against `keleusma = "=0.2.2"`, exposing `__version__`, `KELEUSMA_VERSION` and
  `BYTECODE_VERSION`. Smoke tests, the design, the 0.1.0 milestone and the process files.
