# pykeleusma

Python bindings for [Keleusma](https://github.com/sgeos/keleusma), a total functional
stream processor whose programs have statically bounded worst-case execution time and
memory use. The goal is to run Keleusma scripts inside a Python program, for example
to drive a PyGame frame loop or to compute alongside NumPy.

The distribution is `pykeleusma` and the import package is `keleusma`, following
wrappers such as `pyzmq`, which imports as `zmq`, and `pycairo`, which imports as
`cairo`.

## Status

**Early development.** The repository was cut on 2026-10-24 and builds against the
published Keleusma 0.2.2 from crates.io. Compiling, verified construction, calling,
resuming, streaming, value conversion, Python natives and shared data work, as do the
book's structs, enums and options, recoverable native errors, bundled libraries,
precompiled, signed and encrypted bytecode, and hot swap. The ten example scripts that
ship with Keleusma run in tests with their stated results, and so do three examples of
this repository's own. Keleusma 0.2.2 defects that the binding works around, and what
it does not support, are in the design under Book coverage. The first milestone is in
[`docs/roadmap/V0_1_0.md`](docs/roadmap/V0_1_0.md) and the design in
[`docs/architecture/BINDING_DESIGN.md`](docs/architecture/BINDING_DESIGN.md). Nothing is
published to PyPI yet.

## What is guaranteed, and what is not

Keleusma proves an upper bound on the time and memory of a script before it runs. That
bound covers **the script, and the bounds declared for each native**. It never covers
Python.

- **Python code is outside every bound.** That includes a Python callable registered as
  a native, the binding's conversion of values, and anything the host does between
  calls. A native can run for any time and allocate any memory.
- **A native's declared bounds are promises, not limits.** The `wcet` and `wcmu_bytes`
  given for a native are attestations the host makes and Keleusma trusts. Keleusma uses
  them to size the arena and to compute `vm.wcet_per_iteration`, but Python does not
  obey them. A script that calls a Python native has a worst case only for its own part.
- **The figures are in Keleusma's units.** `vm.wcet_per_iteration` is the worst case of
  one iteration of a `loop main` in nominal cycles, the cycle costs of Keleusma's
  nominal cost model, which are not the cycles of any particular processor and not
  seconds. `vm.arena_capacity` is the arena's size in bytes.
- **Two limits are empirical in this release.** The arena includes a margin for a
  shortfall in Keleusma 0.2.2's own sizing, measured rather than proven, and composite
  arguments and native results draw on a budget the host chooses, `argument_bytes`.
  The binding also rejects a class of mistyped yields that Keleusma 0.2.2 accepts and
  that could crash a read, but it cannot check every expression. The design document
  records the evidence and the limits.

What the binding does guarantee is that failures surface as Python exceptions. A
verification failure raises `VerifyError`, a runtime fault raises `VMError` and marks
the VM `failed`, and a native's own exception reaches the caller unchanged.

## Using it

```python
import keleusma

module = keleusma.compile("fn main() -> Word { 40 + 2 }")

try:
    keleusma.compile("fn main() -> Word { $ }")
except keleusma.SourceError as error:  # LexError, ParseError or CompileError
    print(error.msg, error.lineno, error.colno)  # unexpected character '$' 1 21

vm = keleusma.VM(module)  # raises keleusma.VerifyError if the verifier rejects it
assert vm.call() == keleusma.Finished(42)

doubler = keleusma.compile("loop main(e: Word) -> Word { let n = yield e * 2; n }")
stream = keleusma.VM(doubler)
match stream.call(1):
    case keleusma.Yielded(value):  # 2
        print(value)
stream.resume(10)  # keleusma.Reset(), the end of one iteration
stream.resume(20)  # keleusma.Yielded(40), the start of the next
```

- **Values** convert by the types `main` declares. `Word` and `Byte` become `int`,
  `bool` stays `bool`, `Float` becomes `float`, `Text` becomes `str`, tuples become
  `tuple`, arrays become `list`, and `Fixed<N>` becomes `keleusma.Fixed`, an exact
  `Fraction` that remembers its width. A struct becomes a `dict` of its fields, an enum
  value a `keleusma.Variant`, and an `Option` either `None` or the value. Out-of-range
  integers raise `OverflowError` instead of wrapping.
- **Streams.** `vm.stream(*args, inputs=...)` drives a VM as a generator. A `loop main`
  reports `Reset` after each iteration, and the next input starts the next one.
- **Natives** are passed as `keleusma.VM(module, natives={"host::f": f})`, or as
  `keleusma.Native(f, wcet=..., wcmu_bytes=...)` to declare bounds, which size the
  arena or are refused.
- **State.** Private data fields start as `()`, not zero, so assign one before reading
  it. A shared `data` block is owned by the VM, which keeps it across calls.
  `vm.call(x, shared=buffer)`, with a `bytearray` of `vm.shared_data_bytes` bytes, syncs
  the block from `buffer`, runs, and flushes the block back to it, and `vm.shared_data`
  returns a copy of it.
- **Arena budget.** A VM reserves `argument_bytes`, 4096 by default, of its arena for
  composite arguments such as tuples carrying text. A host that passes larger values
  gives a larger figure.
- **Native errors.** A native that raises `keleusma.NativeError(code)` hands `code` to
  a script that handles it with `host::f(x) { ok(v) => v, error(code) => ... }`. Any
  other exception fails the call and reaches the caller unchanged.
- **After a runtime error** a VM reports `failed` and refuses further use, because
  Keleusma leaves its state undefined, until `vm.reset()` clears the error and keeps
  the data segment.
- **Libraries.** `VM(module, libraries=["math"])` offers Keleusma's bundled `math`,
  `audio` or `shell` natives, and `keleusma.library_signatures("math")` gives the
  declarations a script includes.

### Bytecode, signing and hot swap

```python
import keleusma

seed = bytes(range(32))  # an Ed25519 signing seed, kept secret by whoever signs
trusted = keleusma.signing_public_key(seed)

source = "signed loop main(t: Word) -> Word { let n = yield t + 1; n }"
data = keleusma.compile(source).to_signed_bytes(seed)  # bytes for a .kel.bin file
update = keleusma.load_module(data, verifying_keys=[trusted])

baseline = keleusma.compile("loop main(t: Word) -> Word { let n = yield t * 10; n }")
vm = keleusma.VM(baseline)
vm.register_verifying_key(trusted)  # from now on, only signed code may be swapped in
vm.call(1)
vm.resume(0)  # keleusma.Reset(), an iteration boundary
vm.replace_module(update)
assert vm.call(1) == keleusma.Yielded(2)
```

A module loaded from bytes has no declared types, so only scalars and text cross the
boundary. `to_signed_bytes(seed, recipient_key=...)` also encrypts the module, and
`load_module` then needs the matching `decryption_key`. A swap happens only at an
iteration boundary, keeps shared data and starts private data afresh, and the VM is
built from the module with the larger footprint, because its arena does not grow.

```python
import keleusma

keleusma.KELEUSMA_VERSION  # "0.2.2"
keleusma.BYTECODE_VERSION  # 1, read from the linked runtime
```

## Examples

Three runnable examples live in `examples/`, and `tests/test_examples.py` runs each one.

```sh
uv run python examples/batched_update_loop.py   # 100 session lines, one thread, a tick budget
uv run python examples/numpy_reduction.py       # a NumPy mean and variance, 8 samples per step
uv run python examples/pygame_frame_loop.py     # a bouncing ball in a window
```

The PyGame example opens a window, which is the documented manual check. The tests run
it headless with `SDL_VIDEODRIVER=dummy`. Each frame advances the script by one stream
iteration, as the example explains.

## Interface at a glance

Every part of this sketch is implemented, as the target for version 0.1.0.

```python
import keleusma

# Natives declared at construction size the arena with their memory bounds.
vm = keleusma.VM(module, natives={"host::draw": keleusma.Native(draw, wcet=50)})

for state in vm.stream(first_event, inputs=events):
    match state:
        case keleusma.Yielded(actions):
            ...  # perform the actions
        case keleusma.Reset():
            ...  # one iteration done; the next event starts the next one
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
