# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Shared `data` blocks, owned by the VM in Rust and lent to Keleusma on each call.
  `shared=` on `vm.call`, `vm.resume` and `vm.stream` syncs the block from a Python
  buffer and flushes it back after the call. `vm.shared_data_bytes` and
  `vm.shared_data` describe and copy it.
- The README's section "What is guaranteed, and what is not", stating that Keleusma's
  bounds cover the script and declared native bounds, never Python.

- Three examples in `examples/`, a PyGame frame loop, a NumPy reduction and a batched
  update loop in Dromon's event and action shape, each run by a test, with NumPy and
  `pygame-ce` added to the development dependencies.

- Python natives through `VM(module, natives=...)`, `vm.register`,
  `vm.set_native_bounds` and `vm.native_bounds`, with `keleusma.Native` for declared
  bounds. Memory bounds size the arena or are refused, a native's exception reaches the
  caller unchanged, and cycles through natives are collected.

- `vm.stream(*args, inputs=None)`, a generator yielding `Yielded` and `Reset` states and
  returning the `Finished` value, with resume inputs from an iterable or from `send`.

- `vm.wcet_per_iteration`, Keleusma's worst-case time for one stream iteration in its
  nominal cost units, or `None` for a module without a stream, with the units and the
  coverage of it and of `vm.arena_capacity` documented.

- `vm.call(*args)` and `vm.resume(value)`, returning the frozen dataclasses
  `keleusma.Yielded`, `Finished` and `Reset`, and `vm.failed`, set after a runtime error
  leaves the VM's state undefined. They replace the private `VM._call` test hook.

- Value conversion by the entry point's declared types, covering `()`, `bool`, `Byte`,
  `Word`, `Float`, `Text`, `Fixed<N>`, tuples and arrays, and `keleusma.Fixed`, an exact
  `Fraction` subclass carrying its fraction bits.
- A compile-time check that every yield matches the declared return type, because
  Keleusma 0.2.2 accepts some mistyped yields that crash a later read.

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
