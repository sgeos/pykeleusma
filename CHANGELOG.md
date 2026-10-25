# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `keleusma.VM(module, *, argument_bytes=4096)`, a verified VM that owns an arena sized
  to survive `Reset` and to hold composite arguments, with `arena_capacity` and
  `argument_bytes` properties. The exceptions `VerifyError` and `VMError`, and the
  constant `DEFAULT_ARGUMENT_BYTES`. It replaces the temporary `_OwnedVmProbe`.

- `keleusma.compile(source)`, returning an immutable `keleusma.Module`, and the
  exceptions `KeleusmaError`, `SourceError`, `LexError`, `ParseError` and
  `CompileError`. A source error carries `msg`, `lineno` and a character-based `colno`.
- Continuous integration on GitHub Actions, testing Linux and macOS with Python 3.12
  and 3.15, plus the Rust tests under Miri.

- `OwnedVm`, which owns a Keleusma arena and the VM that borrows it as one `self_cell`
  value, sizes the arena from the module, and raises `MemoryError` rather than aborting
  when the arena cannot be allocated. A temporary private class `_OwnedVmProbe` and a
  live-VM counter exercise it from Python until `keleusma.VM` arrives.
- Ignored project-local `tmp/` and `secret/` directories, following the Keleusma
  repository.

### Fixed

- An arena sized by Keleusma's footprint alone failed on the resume after a stream's
  first `Reset`. The arena now includes a regrowth margin, as the design records.

- `uv sync` now rebuilds the extension after a change to the Rust sources, through
  `cache-keys` in `pyproject.toml`. Before, it kept a stale build.

- The repository seed. A PyO3 extension module `keleusma._keleusma`, built by maturin
  against `keleusma = "=0.2.2"`, exposing `__version__`, `KELEUSMA_VERSION` and
  `BYTECODE_VERSION`. Smoke tests, the design, the 0.1.0 milestone and the process files.
