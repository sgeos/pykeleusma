# pykeleusma

Python bindings for [Keleusma](https://github.com/sgeos/keleusma), a total functional
stream processor whose programs have statically bounded worst-case execution time and
memory use. The goal is to run Keleusma scripts inside a Python program, for example
to drive a PyGame frame loop or to compute alongside NumPy.

The distribution is `pykeleusma` and the import package is `keleusma`, following
wrappers such as `pyzmq`, which imports as `zmq`, and `pycairo`, which imports as
`cairo`.

## Status

**Early development.** Version 0.2.2.0 builds against the published Keleusma 0.2.2
from crates.io. Compiling, verified construction, calling, resuming, streaming, value
conversion, Python natives and shared data work, as do the book's structs, enums and
options, big numbers, opaque host values, recoverable native errors, bundled libraries,
precompiled, signed and encrypted bytecode, hot swap and custom cost models. The ten
example scripts that ship with Keleusma run in tests with their stated results, and so
do three examples of this repository's own. Nothing is published to PyPI yet.

The design is
[`docs/architecture/BINDING_DESIGN.md`](https://github.com/sgeos/pykeleusma/blob/main/docs/architecture/BINDING_DESIGN.md),
the first milestone is
[`docs/roadmap/V0_1_0.md`](https://github.com/sgeos/pykeleusma/blob/main/docs/roadmap/V0_1_0.md),
and changes are listed in
[`CHANGELOG.md`](https://github.com/sgeos/pykeleusma/blob/main/CHANGELOG.md).

## Limitations

**This is a best-effort binding for Keleusma 0.2.2.** Most of its limitations come
from defects and gaps in that Keleusma release, which have been reported to the
Keleusma project. **They are expected to clear when the public Keleusma 0.2.3 is
released.** Some are already fixed on Keleusma's development branch. The rest have not
been confirmed fixed, so each will be checked when the binding moves to 0.2.3, and any
that remains will be listed then. The move is planned in
[`docs/architecture/MIGRATION_V0_2_3.md`](https://github.com/sgeos/pykeleusma/blob/main/docs/architecture/MIGRATION_V0_2_3.md).

Limitations from Keleusma 0.2.2, expected to clear with 0.2.3.

- **Arena sizing is measured, not proven.** Keleusma 0.2.2 sizes the arena too small
  after a stream's first `Reset`, so the binding adds a margin found by measurement.
  Composite arguments, such as tuples carrying text, draw on a budget the host chooses,
  `argument_bytes`, rather than on a computed bound.
- **Yield types are checked only in part.** Keleusma 0.2.2 accepts a yield whose value
  does not match the declared type, and reading such a value can crash the process.
  The binding refuses every mistyped yield it can detect and every text-bearing yield
  whose type Keleusma did not record, but it cannot check every expression.
- **Private data starts as `()`, not zero,** although the book says zero. Assign a
  private field before reading it.
- **Assign a private array element by element.** In 0.2.2, `pv.a = [x, y]` fills only
  the first element's slot, so a later `pv.a[1]` reads `()`, while `pv.a[0] = x` and
  `pv.a[1] = y` work.
- **Generic types cross in one direction only.** A type-generic struct or a
  const-generic enum can be returned but not passed in, and a type-generic enum is not
  supported, because Keleusma 0.2.2 lays out their instances inconsistently.
- **Hot swap cannot seed composite private fields.** Only scalar fields and arrays of
  scalars can be carried into a new module.
- **`Text + Text` fails at run time,** although the book says it works.
- **Library declarations are not checked by Keleusma.** The binding refuses a script
  whose `use` line for a bundled library native differs from the library's signature.

Moving to 0.2.3 will also change one thing for the worse. Keleusma 0.2.3 changes the
bytecode version from 1 to 2 and rejects version 1, so **bytecode saved with this
release will not load after the move**. Keep the source of every script, and compile
it again.

Limitations of the binding's own design, which 0.2.3 does not change.

- **Python is outside every bound,** as the next section explains.
- **A VM belongs to the thread that creates it.** A compiled `Module` may be shared
  between threads.
- **Bytecode loaded from bytes has no declared types,** so only scalars and text cross
  its boundary.
- **Some shapes are refused as inputs.** Nested options, whose `None` would be
  ambiguous, and refined newtypes, whose rule is script code the host cannot run.
- **Keys passed from Python cannot be wiped.** A signing seed or decryption key lives
  in a Python `bytes` object, which the binding cannot zero.
- **Shared data is copied.** A NumPy array cannot yet serve as a VM's shared data
  block without a copy.

## What is guaranteed, and what is not

Keleusma proves an upper bound on the time and memory of a script before it runs. That
bound covers **the script, and the bounds declared for each native**. It never covers
Python.

- **Python code is outside every bound.** That includes a Python callable registered as
  a native, the binding's conversion of values, Python objects carried as opaque values,
  and anything the host does between calls. A native can run for any time and allocate
  any memory.
- **A native's declared bounds are promises, not limits.** The `wcet` and `wcmu_bytes`
  given for a native are attestations the host makes and Keleusma trusts. Keleusma uses
  them to size the arena and to compute `vm.wcet_per_iteration`, but Python does not
  obey them. A script that calls a Python native has a worst case only for its own part.
- **The figures are in Keleusma's units.** `vm.wcet_per_iteration` is the worst case of
  one iteration of a `loop main` in nominal cycles, the cycle costs of Keleusma's
  nominal cost model, which are not the cycles of any particular processor and not
  seconds. `vm.arena_capacity` is the arena's size in bytes.
- **Two limits are empirical in this release.** The arena margin and the check of
  yield types, both described under Limitations. The design document records the
  evidence.

What the binding does guarantee is that failures surface as Python exceptions. A
verification failure raises `VerifyError`, a runtime fault raises `VMError` and marks
the VM `failed`, and a native's own exception reaches the caller unchanged.

## Installing

```sh
pip install pykeleusma
```

Prebuilt wheels cover Linux with glibc and with musl, such as Alpine, macOS, and
Windows, each on x86-64 and ARM64. On each, one wheel serves CPython 3.12 and later
through the stable application binary interface, and free-threaded CPython 3.14t and
3.15t, which run without the global interpreter lock, get a wheel each. Elsewhere, pip
builds from the source distribution, which needs a Rust toolchain of at least 1.88.

A VM belongs to the thread that creates it, and a compiled `Module` may be shared
between threads. That holds with and without the global interpreter lock.

The version is the Keleusma release the binding is built against, followed by a fourth
part that counts binding releases against it. Version 0.2.2.0 is the first binding
release for Keleusma 0.2.2, and `keleusma.KELEUSMA_VERSION` reports the linked release.
Bytecode saved by one Keleusma release may not load under another, as described under
Relationship to Keleusma.

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
  value a `keleusma.Variant`, and an `Option` either `None` or the value. A
  `Multiword<N, F>` big number becomes an `int`, or an exact `Fraction` when `F` is not
  zero. A type the script names but does not define is an opaque host type, and any
  Python object passes through it unchanged. Out-of-range integers raise
  `OverflowError` instead of wrapping.
- **Streams.** `vm.stream(*args, inputs=...)` drives a VM as a generator. A `loop main`
  reports `Reset` after each iteration, and the next input starts the next one.
  `vm.step(value)` advances one iteration in a single call, for a frame loop.
- **Natives** are passed as `keleusma.VM(module, natives={"host::f": f})`, or as
  `keleusma.Native(f, wcet=..., wcmu_bytes=...)` to declare bounds, which size the
  arena or are refused. A native the script imports with `use external` needs
  `keleusma.Native(f, max_invocations=n)`, the attested calls per iteration.
- **State.** Private data fields start as `()`, not zero, so assign one before reading
  it. A shared `data` block is owned by the VM, which keeps it across calls.
  `vm.call(x, shared=buffer)`, with a `bytearray` of `vm.shared_data_bytes` bytes, syncs
  the block from `buffer`, runs, and flushes the block back to it, and `vm.shared_data`
  returns a copy of it. `vm.read_shared()` and `vm.write_shared({"field": value})`
  convert the block's fields by their declared types.
- **Arena budget.** A VM reserves `argument_bytes`, 4096 by default, of its arena for
  composite arguments such as tuples carrying text. A host that passes larger values
  gives a larger figure. `vm.arena_peak_bytes` shows the most the VM has used.
- **Native errors.** A native that raises `keleusma.NativeError(code)` hands `code` to
  a script that handles it with `host::f(x) { ok(v) => v, error(code) => ... }`. Any
  other exception fails the call and reaches the caller unchanged.
- **After a runtime error** a VM reports `failed` and refuses further use, because
  Keleusma leaves its state undefined, until `vm.reset()` clears the error and keeps
  the data segment.
- **Libraries.** `VM(module, libraries=["math"])` offers Keleusma's bundled `math`,
  `audio` or `shell` natives, and `keleusma.library_signatures("math")` gives the
  declarations a script includes. A script's declaration of a library native must
  match the library's.
- **Other cost models.** `vm.wcet_per_iteration_with({"CheckedAdd": 3})` computes the
  worst case under a cost table of the host's, keyed by the names in
  `keleusma.OP_NAMES`, with every other opcode at its nominal cost.

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
boundary. **Bytecode is code.** Its checksum catches corruption, not tampering, and
Keleusma's decoder is not known to be robust against crafted bytes, so load unsigned
bytecode only from a source as trusted as the script's own source. A signature tells
the host who produced the bytes. It protects only as far as the holders of the trusted
keys are trusted. `to_signed_bytes(seed, recipient_key=...)` also encrypts the module, and
`load_module` then needs the matching `decryption_key`. A swap happens only at an
iteration boundary, keeps shared data and starts private data afresh, unless
`private={"field": value}` seeds the new module's scalar private fields, and
`vm.private_data` reads the running module's private fields, so state can be carried. The VM is
built from the module with the larger footprint, because its arena does not grow.

```python
import keleusma

keleusma.KELEUSMA_VERSION  # "0.2.2"
keleusma.BYTECODE_VERSION  # 1, read from the linked runtime
```

## Examples

Three runnable examples live in the repository's `examples/` directory, and
`tests/test_examples.py` runs each one.

```sh
uv run python examples/batched_update_loop.py   # 100 session lines, one thread, a tick budget
uv run python examples/numpy_reduction.py       # a NumPy mean and variance, 8 samples per step
uv run python examples/pygame_frame_loop.py     # a bouncing ball in a window
```

The PyGame example opens a window, which is the documented manual check. The tests run
it headless with `SDL_VIDEODRIVER=dummy`. Each frame advances the script by one stream
iteration, as the example explains.

## Interface at a glance

The shape of a host loop that drives a stream with events.

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
[`docs/decisions/STANDALONE_REPOSITORY.md`](https://github.com/sgeos/pykeleusma/blob/main/docs/decisions/STANDALONE_REPOSITORY.md).

## License

[0BSD](https://github.com/sgeos/pykeleusma/blob/main/LICENSE), the same as Keleusma.
The wheels also contain Rust crates under their own licenses, whose notices are
reproduced in
[`THIRD_PARTY_LICENSES.txt`](https://github.com/sgeos/pykeleusma/blob/main/THIRD_PARTY_LICENSES.txt).
