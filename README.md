# pykeleusma

Python bindings for [Keleusma](https://github.com/sgeos/keleusma), a total functional
stream processor whose programs have statically bounded worst-case execution time and
memory use. The goal is to run Keleusma scripts inside a Python program, for example
to drive a PyGame frame loop or to compute alongside NumPy.

The distribution is `pykeleusma` and the import package is `keleusma`, following
wrappers such as `pyzmq`, which imports as `zmq`, and `pycairo`, which imports as
`cairo`.

## Status

**Seed only.** The repository was cut on 2026-10-24. The extension builds, links the
published Keleusma 0.2.2 from crates.io, and exposes version information. No script
can run yet. The first milestone is in [`docs/roadmap/V0_1_0.md`](docs/roadmap/V0_1_0.md)
and the planned interface in
[`docs/architecture/BINDING_DESIGN.md`](docs/architecture/BINDING_DESIGN.md).

```python
import keleusma

keleusma.KELEUSMA_VERSION  # "0.2.2"
keleusma.BYTECODE_VERSION  # 1, read from the linked runtime
```

## Planned interface

Not implemented. This is the target for version 0.1.0.

```python
import keleusma

module = keleusma.compile(source)  # raises keleusma.CompileError with a location
vm = keleusma.VM(module)  # verifies bounds and sizes the arena
vm.register("draw", lambda x, y: ...)  # a Python callable as a host native

for frame in vm.stream():  # one resume per yield of the script's loop
    ...
```

## Building

The build needs a Rust toolchain of at least 1.88 and [uv](https://docs.astral.sh/uv/).

```sh
uv sync          # builds the extension with maturin and installs the dev tools
uv run pytest
```

## Relationship to Keleusma

This repository depends only on the published crate, pinned exactly in `Cargo.toml`.
It never builds against a Keleusma checkout and never edits the Keleusma repository.
Moving to a later Keleusma release is a deliberate change, because 0.2.3 changes the
bytecode version from 1 to 2. The reasons for a separate repository are in
[`docs/decisions/STANDALONE_REPOSITORY.md`](docs/decisions/STANDALONE_REPOSITORY.md).

## License

[0BSD](LICENSE), the same as Keleusma.
