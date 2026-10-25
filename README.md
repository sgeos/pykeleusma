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
published Keleusma 0.2.2 from crates.io. Compiling a script, building a verified VM,
calling and resuming it, streaming it as a generator, and Python natives work. Runnable
examples are in `examples/`. The full statement of the guarantee boundary and a release
are still to come, as the first milestone lists.

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

Values convert by the types `main` declares. `Word` and `Byte` become `int`, `bool`
stays `bool`, `Float` becomes `float`, `Text` becomes `str`, tuples become `tuple`,
arrays become `list`, and `Fixed<N>` becomes `keleusma.Fixed`, an exact `Fraction` that
remembers its width. Out-of-range integers raise `OverflowError` instead of wrapping.
After a runtime error a VM reports `failed` and refuses further use, because Keleusma
leaves its state undefined. Private data fields start as `()`, not zero, so assign one
before reading it. A module with a shared `data` block is refused, because the binding
does not yet lend it the host buffer Keleusma requires.

For scheduling, `vm.wcet_per_iteration` gives the worst-case time of one iteration of a
`loop main` in Keleusma's nominal cost units, which are not cycles or seconds, and
`vm.arena_capacity` gives the arena's size in bytes. Both cover the script and the
declared bounds of natives only, never Python or the binding's conversions.

**Natives are outside the guarantee.** A Python callable registered as a native has no
time or memory bound. The `wcet` and `wcmu_bytes` you declare for it are attestations
Keleusma trusts and uses for sizing, not limits Python obeys. A native's exception
reaches the caller of `call` or `resume` unchanged.

A VM reserves `argument_bytes`, 4096 by default, of its arena for composite arguments
such as tuples carrying text. A host that passes larger values gives a larger figure. The first milestone is in [`docs/roadmap/V0_1_0.md`](docs/roadmap/V0_1_0.md)
and the planned interface in
[`docs/architecture/BINDING_DESIGN.md`](docs/architecture/BINDING_DESIGN.md).

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
