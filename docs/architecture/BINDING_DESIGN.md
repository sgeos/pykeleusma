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
| `keleusma.VM(module)` | `Arena::try_with_capacity` sized as described under Arena sizing, then `Vm::new(module, &arena)`, which verifies the module |
| `vm.register(name, callable)` | `Vm::register_native_closure(name, func)`, where `func` is `Fn(&[Value]) -> Result<Value, VmError> + 'static` |
| `vm.call(*args)` | `Vm::call(&[Value])`, returning a `VmState` |
| `vm.resume(value)` | `Vm::resume(Value)` |
| `vm.stream(*args)` | A Python generator over `call` and then `resume` while the state is `Yielded` |
| Result kinds | `VmState::Yielded`, `Finished`, `Reset`, and a breakpoint state the binding does not expose in 0.1.0 |
| Exceptions | `LexError`, `ParseError` and `CompileError`, each with a `Span` of `line` and `column`, implemented as described under Compilation, and `VmError`, raised as `VerifyError` or `VMError` as described under Arena sizing |

`Vm` is `GenericVm<'a, 'arena, i64, u64, f64>` and `Value` is `GenericValue<i64, f64>`.
The `'a` lifetime belongs to the bytecode store and borrows input bytes only for
zero-copy loading. A VM built from an owned `Module` can take `'static` there.

## Compilation and source errors

**Implemented on 2026-10-25, roadmap task 2.** `keleusma.compile(source)` runs
Keleusma's pipeline in `src/compile.rs` with the GIL released, and returns an immutable
`keleusma.Module`, which may be shared between threads because Keleusma's `Module` is
`Send` and `Sync`.

| Python | Raised when |
|---|---|
| `KeleusmaError` | Base class for every error the package raises |
| `SourceError(KeleusmaError, ValueError)` | Base class for rejected source, with `msg`, `lineno` and `colno` |
| `LexError`, `ParseError`, `CompileError` | The lexer, parser or compiler rejected the source |

- **Why Python classes.** The classes live in `python/keleusma/_errors.py`, because a
  source error derives from both `KeleusmaError` and `ValueError`, which PyO3's
  exception macros cannot express. The attribute names and the `ValueError` base follow
  `json.JSONDecodeError`, the standard library's precedent for a located input error.
- **Columns.** Keleusma's column counts bytes. The binding converts it to a character
  column from the source text, so a column after non-ASCII text matches what a Python
  editor shows. If the span does not fit the source, the byte column is kept rather than
  failing while reporting a failure.
- **Unknown location.** Keleusma uses line 0 for synthesised types, and the binding then
  reports `None` for both line and column.
- **Not covered.** No test shows that the GIL is released during compilation. Keleusma
  does not bound compilation time, so a very large source still takes as long as it
  takes, but other threads keep running.

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
Borrows, including Miri's leak check. `tests/test_vm.py` creates and drops 20,000 VMs,
through `keleusma.VM` since task 3 and a temporary probe class before it, and compares
peak resident memory with a control that keeps as many alive. A deliberate mutation that leaked one VM per
construction made that test fail, so the test can detect the leak it guards against.

## Arena sizing, roadmap task 3

**Implemented on 2026-10-25.** `keleusma.VM(module, *, argument_bytes=4096)` builds the
arena from four parts, computed in `ArenaPlan::for_module` in `src/owned_vm.rs`.

| Part | Source | Status |
|---|---|---|
| Footprint | `vm::auto_arena_capacity_for(module, &[])` | Keleusma's verified figure |
| Persistent region | `vm::required_persistent_capacity_for(module)`, reserved before the VM is built | Added, because Keleusma does not say whether the footprint includes it |
| Regrowth | Twice the working set, which is the footprint less `module.aux_arena_bytes` and the footprint's heap bytes | Empirical, not proven |
| Argument budget | `argument_bytes`, chosen by the host | Host responsibility |

**Fact, the defect.** In Keleusma 0.2.2 a stream in an arena of exactly the footprint
fails with `OutOfArena` on the resume after its first `Reset`, and a composite argument
also needs space the footprint leaves out. The failure is a clean error. A Rust test
pins the first behaviour, so a fix in Keleusma will show up as a failing test rather
than as a silently oversized arena. Both were reported to `v0.2.3`, whose interim reply
called a host margin a workaround, not a supported bound.

**Measurements, on 2026-10-25, macOS aarch64.** A value slot is 32 bytes and a call
frame 24, which reproduces the footprint exactly, for example 5 slots and 1 frame give
184 bytes. The smallest arena that survived 2,000 iterations was, in every script with
word arguments, the footprint plus one doubling of the operand stack. For the minimal
stream that is 184 + 320 = 504 bytes. The regrowth part covers this with at least 48
bytes to spare in every script measured, which included nested calls, deep
expressions, private data and four yields per iteration.

**Inference.** The restart after a `Reset` needs more operand slots than the verifier's
footprint reserves, and the operand stack then grows inside a bump allocator, where the
abandoned smaller buffer is not reclaimed until a full reset. One doubling is what was
observed. A script that regrew twice would need more, and none was found.

**Arguments.** A tuple argument costs about twice its text length plus a few tens of
bytes, and accumulates across the yields of one stream iteration, at 2L + 22 bytes for
one yield and 3L + 57 for two yields with text of length L. That depends on the host's
data, so no figure derived from the module can cover it. `argument_bytes` is the host's
statement of how much to reserve, 4096 bytes by default. An argument that does not fit
fails with `OutOfArena`, which the binding raises as `keleusma.VMError`.

**Evidence.** Rust tests drive seven word streams and one tuple stream for 10,000
iterations in an automatically sized arena. A mutation that removed the regrowth part
made the word-stream tests fail. The arena is allocated with `Arena::try_with_capacity`,
so an allocation failure raises `MemoryError` instead of aborting the process.

**Construction errors.** `VerifyError(KeleusmaError, ValueError)` carries the verifier's
message, which names the chunk and the failure. Every other Keleusma error during
construction or a run raises `VMError(KeleusmaError, RuntimeError)`. A capacity that
overflows raises `OverflowError`.

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
pinned by `tests/test_vm.py`.

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
