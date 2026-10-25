# Binding Design

> **Navigation**: [Architecture](./README.md) | [Documentation Root](../README.md)

The shape of the binding, first written when the repository was cut on 2026-10-24.
Every Keleusma API named here was read from the `v0.2.2` tag of the Keleusma repository
or from the published 0.2.2 crate. Each section says whether it is implemented, decided
or still a plan, and the risks are stated where the plan rests on an assumption.

## Layers

| Layer | Path | Role |
|---|---|---|
| Rust extension | `src/lib.rs`, built as `keleusma._keleusma` | Owns the Keleusma objects and converts values |
| Python package | `python/keleusma/` | The public, typed interface, re-exporting the extension |
| Type stub | `python/keleusma/_keleusma.pyi` | Types for the extension, kept in step with `src/lib.rs` |

The extension uses PyO3 with the stable ABI from Python 3.12, so one wheel per platform
serves every later Python. maturin builds it.

## Mapping onto Keleusma 0.2.2

| Python | Keleusma 0.2.2 |
|---|---|
| `keleusma.compile(source)` | `lexer::tokenize`, then `parser::parse`, then `compiler::compile`, the pipeline `examples/piano_roll.rs` uses |
| `keleusma.VM(module)` | `Arena::with_capacity` sized by `vm::auto_arena_capacity_for`, then `Vm::new(module, &arena)`, which verifies the module |
| `vm.register(name, callable)` | `Vm::register_native_closure(name, func)`, where `func` is `Fn(&[Value]) -> Result<Value, VmError> + 'static` |
| `vm.call(*args)` | `Vm::call(&[Value])`, returning a `VmState` |
| `vm.resume(value)` | `Vm::resume(Value)` |
| `vm.stream(*args)` | A Python generator over `call` and then `resume` while the state is `Yielded` |
| Result kinds | `VmState::Yielded`, `Finished`, `Reset`, and a breakpoint state the binding does not expose in 0.1.0 |
| Exceptions | `LexError`, `ParseError`, `CompileError`, each with a `Span` of `line` and `column`, and `VmError`, which has a `category()` |

`Vm` is `GenericVm<'a, 'arena, i64, u64, f64>` and `Value` is `GenericValue<i64, f64>`.
The `'a` lifetime belongs to the bytecode store and borrows input bytes only for
zero-copy loading. A VM built from an owned `Module` can take `'static` there.

## Risk 1, the VM borrows its arena

`Vm::new` takes `&'arena Arena`, and a PyO3 class cannot hold a borrowed lifetime. The
binding must own the arena and the VM together.

- **Implemented on 2026-10-24.** `OwnedVm` in `src/owned_vm.rs` is a `self_cell` 1.3.0
  cell whose owner is the `Arena` and whose dependent is the `Vm` borrowing it. The
  module documentation there states the invariants, which are drop order, address
  stability, verified admission only and no cross-thread use.
- **Rejected.** `Box::leak` of the arena. It is simple, but it leaks one arena for every
  VM a Python program creates, which a long PyGame session would do repeatedly.

The drop order matters concretely. The VM's own `Drop` walks the arena's persistent
region to drop private data slots, so the arena must outlive the VM, which `self_cell`
guarantees by dropping the dependent first.

**Evidence.** The Rust tests in `src/owned_vm.rs` pass natively and under Miri with Tree
Borrows, including Miri's leak check. `tests/test_owned_vm.py` creates and drops 20,000
VMs through the temporary `_OwnedVmProbe` class and compares peak resident memory with a
control that keeps as many alive. A deliberate mutation that leaked one VM per
construction made that test fail, so the test can detect the leak it guards against.

**Defect found on 2026-10-25.** The capacity below is too small. In Keleusma 0.2.2 the
resume after the first `Reset` fails with `OutOfArena` in an arena of exactly the auto
size, and so does a call with a composite argument. Peak use was 504 bytes against an
auto size of 184 for a minimal stream. The failure is a clean error. Roadmap task 3 fixes
the sizing, and the method waits on the `v0.2.3` reply to
`pykeleusma-20261025-dynamic-host-findings`.

**Arena sizing as implemented.** The capacity is `vm::auto_arena_capacity_for(module, &[])` plus
`vm::required_persistent_capacity_for(module)`, and the persistent region is reserved
before the VM is built. Keleusma 0.2.2 does not state whether the footprint already
includes the persistent region, so the two are added, at a cost of at most one
persistent region of spare capacity. The arena is allocated with
`Arena::try_with_capacity`, so an allocation failure raises `MemoryError` instead of
aborting the process.

## Risk 2, Python natives are not bounded

A native registered with `register_native_closure` receives the default bounds,
`DEFAULT_NATIVE_WCET` of 10 cost units and a default memory figure, and
`Vm::set_native_bounds` can change them. A Python callable has no bound at all, since it
can run for any time and allocate any memory.

The binding must state plainly that Keleusma's worst-case guarantees cover the script and
the bounds declared for each native, never the Python code behind a native.

**Decided on 2026-10-24 by the operator.** Version 0.1.0 exposes
`Vm::set_native_bounds` for natives written in Python. The planned surface is optional
keyword arguments on registration, `vm.register(name, callable, *, wcet=None,
wcmu_bytes=None)`, together with a method `vm.set_native_bounds(name, wcet, wcmu_bytes)`
that mirrors the Rust call. Omitted bounds keep Keleusma's defaults. Both values are
checked to fit `u32` and raise `OverflowError` otherwise. The documentation must call the
values attestations that the host makes and Keleusma trusts, which Python code cannot
itself honour.

**The arena must cover the declared bounds.** A native's memory bound adds to the
module's worst-case memory use, so it must be known when the arena is sized.

- **Fact.** Keleusma 0.2.2 provides two mechanisms. `auto_arena_capacity_for(module,
  native_wcmu)` sizes an arena from per-native bounds, given in the order of the module's
  `native_names` table, as the Keleusma cookbook recipe "Auto-sizing the arena from the
  module" describes. `Vm::verify_resources()` re-checks the module against the arena
  after natives are registered and their bounds set, as `examples/wcmu_attestation.rs`
  does.
- **Fact.** Neither `call` nor `resume` runs `verify_resources` by itself. A host that
  raises a bound after construction without calling it runs with an arena the verifier
  never admitted.
- **Planned for task 7.** Natives and their bounds are declared when the VM is built,
  as in `keleusma.VM(module, natives={"host::draw": draw})`, where a value is either a
  bare callable with Keleusma's default bounds or `keleusma.native(callable, wcet=...,
  wcmu_bytes=...)`. The binding computes the bound slice in `native_names` order, sizes
  the arena with it, builds and registers, and then calls `verify_resources` as a check
  of its own sizing. `vm.register` and `vm.set_native_bounds` after construction remain
  for compatibility with the Rust interface, and each runs `verify_resources`, raising
  if the existing arena cannot hold the new bound. No path leaves a bound uncovered.
- **Inference.** Opaque host values do not change this. In an arena-resident composite,
  Keleusma stores an opaque as an index into a registry on the VM, not as the
  `Arc<dyn HostOpaque>` fat pointer itself, so its arena cost is one word. Opaques are
  outside 0.1.0 in any case.

## Values

Converted eagerly in both directions, because in 0.2.2 a composite result lives in the
VM's arena and must be read before the next `resume` or before the VM is dropped.

| Keleusma | Python |
|---|---|
| `Unit` | `None` |
| `Bool` | `bool` |
| `Int`, 64-bit | `int`, with `OverflowError` on the way in |
| `Byte` | `int` in 0 to 255, checked on the way in |
| `Float`, 64-bit | `float` |
| `StaticStr` and strings | `str` |
| Tuples and arrays | `tuple` and `list` |
| `Fixed` | `keleusma.Fixed`, an immutable exact value, as described below |
| Structs and enums | Open, possibly dictionaries or generated classes, after 0.1.0 |

### Composite values

**Fact.** Keleusma 0.2.2 stores a tuple or array of scalars and literal strings as flat
bytes with no per-field types. `TupleBody::elements()` panics on that form, so the binding
must never call it on a value it has not checked. `ChunkSignature` records only the kind
and byte size of a flat composite.

**Planned for task 4.** Decode by the entry point's declared parameter and yield types,
read from the parsed program that `keleusma.compile` already holds, and encode arguments
the same way. Whether `value_layout::LayoutDescriptor` matches the compiler's layout is
asked of `v0.2.3`. Until that is answered, round-trip tests of every supported shape are
the evidence.

### Fixed-point values

**Decided on 2026-10-24 by the operator.** `Fixed` converts to a sane Python equivalent,
even if that equivalent carries metadata. The details were left to the binding.

**Fact.** A runtime `Value::Fixed(i64)` carries only the raw bits. The number of fraction
bits is part of the script's type, `Fixed<N>`, and is not stored in the value.

**Planned for task 4.** A frozen class `keleusma.Fixed` with two read-only attributes,
`raw` as an `int` and `frac_bits` as an `int`. Its value is exactly `raw / 2**frac_bits`.
It registers as a `numbers.Rational`, with `numerator`, `denominator`, `__float__`,
`as_fraction()` returning a `fractions.Fraction`, and equality and hashing that agree with
`Fraction` of the same value. Conversion to a float is spelled `float(value)`, through
`__float__`, which is the Python idiom that `Fraction` and `Decimal` follow, rather than
a `to_float()` method. `int(value)`, `round(value)` and `math.floor(value)` work the same
way through the corresponding special methods. That is the standard library's convention for exact
numeric types, so a `Fixed` mixes with `Fraction`, `int` and `float` as a Python user
expects, and nothing is lost on a round trip. `float` was rejected because a 64-bit raw
value does not fit the 53-bit mantissa, and a bare `Fraction` was rejected because it
drops `frac_bits`, which the way back into the VM needs.

**Open risk for task 4.** To return a `Fixed`, the binding must know `N` for the value
being converted. Whether the compiled 0.2.2 module keeps enough type information to find
`N` at a yield or a return is not yet established. If it does not, the fallback is for
the host to state `N` when it calls, which is clumsier, and the question goes to the
`v0.2.3` line first.

## Threading

**Fact.** `Arena` holds `Cell` fields and a raw pointer, so neither it nor the `Vm` that
borrows it is `Send` or `Sync`, and the compiler enforces that for `OwnedVm`. The class
holding one is `#[pyclass(unsendable)]`.

**Deviation from the conventions.** PyO3 enforces `unsendable` with an assertion, so use
from another thread reaches Python as `pyo3_runtime.PanicException`, which derives from
`BaseException` rather than `Exception`. This is the one panic that crosses into Python,
and it originates in PyO3. The alternative, an `unsafe impl Send` with a manual thread
check, was rejected as unsound, because Python can drop the object on any thread, and
PyO3's `unsendable` handles that case where a manual check would not. The behaviour is
pinned by `tests/test_owned_vm.py`.

**Confirmed on 2026-10-24 by the operator.** Keleusma contexts are not thread safe, and
thread-safe programs give each context a dedicated thread. The binding matches that
model. A VM belongs to the thread that creates it, and a test creates and runs VMs on
several worker threads at once.

A native closure must take the GIL before it calls Python. Free-threaded Python is out
of scope for 0.1.0.

## Known issues in dependencies

**Miri with Stacked Borrows.** On 2026-10-24, running the Rust tests under Miri's
default Stacked Borrows model reported undefined behaviour inside rkyv 0.8.18,
`ArchivedStringRepr::check_bytes`, reached from `Vm::new` while validating the archived
module. The report is in Keleusma's dependency, not in this binding, and the same tests
pass under Tree Borrows. It was reported to the `v0.2.3` line, which replied that it is
known. Keleusma's own continuous integration and release gate run Miri under Tree
Borrows for the same reason. The line infers, without having tested it, that 0.2.3 no
longer has this path, because wire format 2 replaced the rkyv auxiliary body. This
repository therefore runs Miri with `MIRIFLAGS=-Zmiri-tree-borrows` for as long as it
depends on 0.2.2.

## Later

- **NumPy without copying.** `Vm::call_with_shared(&mut [u8], ...)` lends the VM a shared
  data buffer for one call. A NumPy array exposed through the buffer protocol could be
  that buffer.
- **Precompiled bytecode.** `Vm::load_bytes`. Keleusma 0.2.2 writes bytecode version 1,
  and 0.2.3 rejects version 1, so bytecode is not portable across that upgrade.
- **Hot swap.** `Vm::replace_module`, useful for live reloading in a game loop.
- **Signed modules.** `Vm::load_signed_bytes` under the `signatures` feature.
- **Wheels for Linux, macOS and Windows,** built in continuous integration with
  maturin's GitHub Action.
