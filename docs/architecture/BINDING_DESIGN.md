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
| Compatibility | `src/compat.rs` | Every Keleusma call whose signature differs on the 0.2.3 branch, as `MIGRATION_V0_2_3.md` describes |

The extension uses PyO3 with the stable ABI from Python 3.12, so one wheel per platform
serves every later Python. maturin builds it.

## Mapping onto Keleusma 0.2.2

| Python | Keleusma 0.2.2 |
|---|---|
| `keleusma.compile(source)` | `lexer::tokenize`, then `parser::parse`, then `compiler::compile`, the pipeline `examples/piano_roll.rs` uses |
| `keleusma.VM(module)` | `Arena::try_with_capacity` sized as described under Arena sizing, then `Vm::new(module, &arena)`, which verifies the module |
| `vm.register(name, callable)` | `Vm::register_native_with_ctx_closure`, once per native at construction, as described under Risk 2 |
| `vm.call(*args)` | `Vm::call(&[Value])`, returning a `VmState`, implemented as described under Calling and resuming |
| `vm.resume(value)` | `Vm::resume(Value)`, implemented likewise |
| `vm.stream(*args, inputs=None)` | A Python generator over `call` and `resume`, as described under Streaming |
| Result kinds | `keleusma.Yielded`, `Finished` and `Reset` for `VmState::Yielded`, `Finished` and `Reset`. The breakpoint state is not exposed, because the binding arms no breakpoints |
| `vm.reset()` | `Vm::reset_after_error`, as described under Book coverage |
| `VM(module, libraries=[...])` | `Vm::register_library` with `stddsl::Math`, `Audio` or `Shell` |
| `module.to_bytes()`, `keleusma.load_module(data)` | `Module::to_bytes`, `Module::from_bytes` and the `wire_format` signing and encryption functions |
| `vm.replace_module(module)` | `Vm::replace_module`, with the trust matrix enforced by the binding, as described under Book coverage |
| `vm.register_verifying_key(key)` | `Vm::register_verifying_key`, mirrored by the binding |
| `vm.read_shared()`, `vm.write_shared(values)` | The binding's flat decoder and encoder, checked against `DataLayout::shared_layout` |
| `vm.wcet_per_iteration_with(op_cycles)` | `verify::module_wcet_with_bounds` with a `CostModel` built from the host's table |
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

## Risk 2, Python natives are not bounded, roadmap task 7

**The guarantee boundary.** Keleusma's worst-case guarantees cover the script and the
bounds declared for each native. A Python callable has no bound, since it can run for
any time and allocate any memory. The declared `wcet` and `wcmu_bytes` are attestations
the host makes and Keleusma trusts, which Python code cannot honour. The binding says so
in `keleusma._native`, the README and this document, and implies nothing stronger.

**Implemented on 2026-10-25,** in `src/natives.rs` and `src/vm.rs`.

| Python | Effect |
|---|---|
| `VM(module, natives={"host::f": f})` | Registers `f` with Keleusma's default bounds, 10 nominal cycles and 0 bytes |
| `VM(module, natives={"host::f": Native(f, wcet=50, wcmu_bytes=256)})` | Registers with declared bounds, and sizes the arena with the memory bound |
| `vm.register(name, f, *, wcet=None, wcmu_bytes=None)` | Replaces the callable, keeping the declared bounds unless new ones are given |
| `vm.set_native_bounds(name, wcet, wcmu_bytes)` | Declares new bounds, or refuses them with `VerifyError` if the arena cannot hold them |
| `vm.native_bounds(name)` | The declared `(wcet, wcmu_bytes)` |

- **Arena sizing.** The memory bounds are passed to `auto_arena_capacity_for` in the
  order of the module's `native_names`, as the Keleusma cookbook recipe "Auto-sizing the
  arena from the module" describes, and `verify_resources` confirms the sizing at
  construction. Neither `call` nor `resume` runs `verify_resources` by itself, so every
  later bound change runs it, and a refused change is reverted in Keleusma as well as in
  the binding's record. No path leaves a bound uncovered.
- **One closure per native.** Each native the module names gets a Keleusma closure at
  construction, which looks the callable up in a registry shared with the VM object.
  Registration therefore updates an entry rather than registering anew, and the VM's
  `__traverse__` lets Python's garbage collector see the callables, so a native bound to
  the object that owns its VM forms a cycle that is collected. A test confirms it.
- **Missing natives.** `call` and `resume` refuse with `VMError` while any native the
  module names has no callable, before the VM runs, so the VM stays usable. Keleusma
  itself would fail inside the VM with `InvalidBytecode`.
- **Exceptions.** A native returns `Result<Value, VmError>`, which cannot carry a Python
  exception, so the closure stores the exception and returns `NativeError`, and the VM
  re-raises the stored exception. The caller receives the very object the native
  raised, including a `KeyboardInterrupt`. The VM is then marked failed, as after any
  runtime error.
- **Types.** A native declared as `use host::f(T1, T2) -> R` converts its arguments and
  result by those types, including composites. A bare `use host::f` converts scalars by
  their runtime tags and refuses composites and `Fixed` with `TypeError`, because their
  layout and fraction bits are unknown without a type.
- **Composite results consume arena.** Keleusma copies a native's composite result,
  with its text, into the arena on every call, which was observed to exhaust an arena of
  the bare footprint. The native's `wcmu_bytes` is the place to declare that size, and
  the argument budget covers it otherwise.
- **Re-entry** into the same VM from a native is refused by PyO3's borrow checking with
  an exception. The registry is borrowed only to clone the callable, so no borrow is
  held while Python runs.
- **External natives.** A native imported with `use external` is attested by its
  calls per stream iteration, given as `keleusma.Native(f, max_invocations=n)`, which
  is required for it and refused for any other native. Keleusma serves one only through
  `register_external_native`, which takes a plain function pointer with no context.
  `src/external.rs` bridges that with a pool of sixteen trampolines. Each run pushes its
  VM's closures and arena onto a thread-local stack, and a trampoline calls the top
  frame's closure with a native context built from that arena, so values convert as
  for any other native. Nested VMs push and pop their own frames. Keleusma 0.2.2 does
  not fold an external native's memory into its worst-case memory bound, so a large
  result from one can exhaust the arena, which raises `VMError`. A native's
  classification persists across a swap, so a swap that reclassifies a native is
  refused. Keleusma was asked for a closure-based form, which would retire the pool.

**Evidence.** 24 tests cover calls, typed and untyped conversion, exception identity,
re-entry, registration, bounds and the collected cycle. Three mutations each made tests
fail: not re-raising, not reverting a refused bound, and not traversing the registry.
The first version of the revert test missed its mutation, because it checked only the
binding's record, and it now also checks the time figure, which reads Keleusma's copy.

**Inference.** Opaque host values do not change the sizing. In an arena-resident
composite, Keleusma stores an opaque as an index into a registry on the VM, so its arena
cost is one word. The book coverage section describes their conversion.

## Calling and resuming, roadmap task 5

**Implemented on 2026-10-25.** `vm.call(*args)` converts each argument by its declared
parameter type, and `vm.resume(value)` converts the value by the type of the stream's
single parameter. Both return a frozen dataclass from `python/keleusma/_state.py`,
`Yielded(value)`, `Finished(value)` or `Reset()`, so a host compares results with `==`
or takes them apart with `match`.

**Facts, observed on Keleusma 0.2.2.**

- A `yield main` yields, resumes and then returns `Finished`. Resuming after that returns
  `NotSuspended`.
- A `loop main` returns `Reset` on the resume that completes an iteration. The next
  resume starts a new iteration with that value as its argument. It never finishes.
- A `fn main` returns `Finished` from `call` and cannot be resumed.
- The VM does not check a resumed value's type. A string resumed into a `Word` stream was
  accepted and returned as the result. The binding's conversion by the parameter type is
  therefore the only check, which is why a resume type is modelled only for an entry
  point with exactly one parameter, matching the verifier's record that a stream resumes
  with its single parameter's type.

**After a runtime error.** Keleusma's execution model says a runtime error during `call`
or `resume` leaves the VM in an undefined intermediate state. The binding sets
`vm.failed` and refuses every later `call` or `resume` with `VMError`. A conversion
error, raised before the VM is entered, and `NotSuspended`, which Keleusma returns before
changing anything, leave the VM usable. Recovery through Keleusma's
`Vm::reset_after_error` is possible future work.

**Evidence.** A coroutine that yields three values runs to completion. 1,000 VMs built
from one module on one thread are each resumed in turn over four ticks. A stream with
tuple arguments runs 10,000 iterations from Python. A mutation that stopped marking a
failed VM made both failure tests fail.

## Streaming, roadmap task 6

**Implemented on 2026-10-25.** `vm.stream(*args, inputs=None)` returns a generator. It
yields every `Yielded` and `Reset` state and returns the `Finished` value, so
`result = yield from vm.stream(...)` or `StopIteration.value` receives it. Iteration
therefore ends on `Finished`.

- **Inputs.** Each resume takes its value from `inputs`, an iterable, or, without one,
  from `generator.send(value)`. The stream ends when `inputs` runs out. `next()` sends
  `None`, which converts only where the parameter is declared as `()`, so a plain `for`
  loop without `inputs` suits exactly those streams. Sending a value while `inputs` is
  given raises `TypeError`, because one of the two would be silently ignored.
- **The meaning of `Reset`.** A `loop main` never finishes. It reports `Reset` after
  each iteration, and the next input starts the next iteration as its argument. The
  generator yields `keleusma.Reset()` at that boundary, so the consumer sees it.
- **Laziness and early exit.** As with any Python generator, nothing runs until the
  generator is first advanced. Leaving early leaves the VM suspended and resumable.
- **Why Python.** `VM.stream` delegates to `keleusma._stream.stream`, because a
  generator is far simpler to write and to review in Python than through PyO3.

**Evidence.** Tests cover a coroutine ending on `Finished` through `for`, `send` and
`yield from`, a loop's `Reset` boundaries, a unit stream in a plain `for` loop, early
exit, and errors. A mutation that kept iterating past `Finished` made five tests fail.

## Data segments, roadmap task 11

Reported by the `pydromon` line on 2026-10-25 as `pydromon-20261025-data-segment-gap`,
and requested on the operator's instruction as `pydromon-20261025-data-segment-request`.

- **Fact, private data starts as `()` and persists.** Keleusma 0.2.2 initialises each
  private slot to `Unit`, as its own source states at `vm.rs:1141` and `vm.rs:2082`, so
  reading a field before assigning it fails at run time. The same failure appeared
  through the Rust API alone in task 1, so the binding is not the cause. Once assigned,
  a private field keeps its value across iterations, so a script that initialises it on
  its first iteration, for example from a flag in its input, keeps state inside the VM.
  The Keleusma book, chapter 18, says the segment begins zeroed, which disagrees, and
  that was sent to `v0.2.3`. A test pins the behaviour.
- **Shared `data` blocks, implemented.** Keleusma lends a module's shared block to each
  call as a host-owned byte buffer, through `call_with_shared` and `resume_with_shared`.
  The VM owns that buffer in Rust for its whole life, zeroed at construction, and lends
  it on every call.
- **Sync, call, flush.** Decided on 2026-10-25 by the operator. A call given `shared=`,
  any writable, contiguous byte buffer of exactly `vm.shared_data_bytes` bytes such as a
  `bytearray`, a `memoryview` of one or a NumPy `uint8` array, syncs the Rust copy from
  it, runs the VM, and flushes the Rust copy back to it. A call without `shared=` runs on
  the Rust copy as it stands, so state persists across calls either way, and
  `vm.shared_data` returns a copy of it. Passing `shared=` to a module without a block
  raises `TypeError` before the VM runs.
- **Python's buffer is never lent.** Whatever Python does to its own buffer during a call,
  for example from a native, cannot affect the run, and the flush overwrites it. In the
  operator's words, that is a matter for the Python code, not for Keleusma. This also
  needs no `unsafe` code. A native sees the Python buffer as it was synced.
- **After a failure** the flush is skipped, so the Python buffer keeps its pre-call
  contents. The VM is then failed and refuses further use.
- **Layout.** Fields are packed little-endian in declaration order, with no padding, as
  observed with `struct` in the tests: `Word` and `Float` take eight bytes, and `bool`
  and `Byte` one. A typed view of the block is future work, because Keleusma's layout
  table expands each array element into its own entry, so names and entries do not map
  one to one.

**Evidence.** 17 tests cover reading, writing, persistence without a buffer, syncing over
the VM's copy, host changes between steps, the layout, streams, other buffer types, every
refusal, a failure, the native's view, and a native's write being overwritten by the
flush. Mutations that skipped the flush, or cleared the Rust copy between calls, made
tests fail.

## Examples, roadmap task 8

**Implemented on 2026-10-25,** in `examples/`, each driven by `tests/test_examples.py`.

| Example | Shows |
|---|---|
| `pygame_frame_loop.py` | A bouncing ball whose physics runs in Keleusma, one stream iteration per frame |
| `numpy_reduction.py` | A mean and variance of a NumPy array, eight samples per iteration, passed as lists |
| `batched_update_loop.py` | A hundred session lines on one thread, each resumed once per tick, with Dromon's tagged tuples and a tick budget |

**Fact, iterations and frames.** A Keleusma 0.2.2 stream must contain a yield, and the
verifier rejects an inner `loop` without a static iteration bound, so an iteration is
one yield and one `Reset`. A host that wants one result per frame or tick therefore
advances one iteration each time. The resume that completes the previous iteration
returns `Reset`, and the next resume starts the new one. `vm.step(value)`, added on
2026-10-25, does both in one call and starts the stream when nothing is running, so the
PyGame example now makes one call per frame.

**Fact, accumulation.** Script bindings are immutable, and private data starts as `()`.
The NumPy example passes its running sums in with each chunk and accumulates the chunk
in private data with `for i in 0..8`, the range form the verifier bounds. Iterating an
array field directly, `for x in e.0`, was rejected as unbounded, and `e.1.0` lexes as
the float `1.0`, so the input tuple is flat.

## Worst-case figures, roadmap task 5b

**Implemented on 2026-10-25.** Two properties give a scheduler, such as Dromon's batched
update loop, the figures it budgets with.

| Property | Unit | Source | Covers |
|---|---|---|---|
| `vm.wcet_per_iteration` | Nominal cycles, an `int` | `Vm::wcet_per_iteration` | One iteration of a `loop main`, the script and the declared bounds of any natives |
| `vm.arena_capacity` | Bytes, an `int` | The arena plan described under Arena sizing | The memory the script itself can use |

- **Time units.** Keleusma 0.2.2 counts time in nominal cycles, the pipelined-cycle
  costs its nominal cost model assigns to each instruction, as the book's chapter 38
  describes. They are not the cycles of any particular processor, and not seconds.
  An earlier revision of this document called the units unitless and not cycles,
  which the operator corrected on 2026-10-25.
- **Other cost models.** `vm.wcet_per_iteration_with(op_cycles, text_byte_cycles=...)`
  applies a cost table of the host's, keyed by the opcode names in `keleusma.OP_NAMES`,
  with every unnamed opcode at its nominal cost. Keleusma 0.2.2 has the `CostModel`
  type and its analysis functions but ships only the nominal table. A measured table,
  such as Keleusma's benchmark crate generates for a processor, can be passed this way.
  `CostModel` takes a plain function pointer, so the table reaches it through a
  thread-local that is set for the duration of the analysis and restored afterwards.
  The binding computes the nominal figure by the same path first and refuses the
  result if it differs from Keleusma's own, which would mean the native bounds were
  assembled differently. An earlier revision said Keleusma has no cost model API,
  which was wrong.
- **Measured use.** `vm.arena_peak_bytes` bounds the most arena memory a VM has used
  at once, the persistent region plus each working end's peak. It lets a host see how
  much of the empirical margin its own scripts use. The crate's example scripts peak at
  most at 23 percent of their arena, mostly because of the argument budget. Exhaustion
  raises `VMError` and never crashes, which the tests cover.
- **No stream, no figure.** Keleusma returns zero for a module without a stream chunk, so
  a `fn main` or a `yield main` would read as free. The binding returns `None` for
  those instead.
- **Not covered.** Neither figure includes Python, the binding's value conversions, the
  arguments' Python objects, or anything the host does between resumes. Keleusma's
  guarantees stop at the script and the bounds declared for natives, and a Python
  callable registered as a native has no bound.
- **Memory caveat.** The arena capacity includes the empirical regrowth margin and the
  host's argument budget, so it is an upper bound on the script's arena use only as far
  as that margin holds, as Arena sizing explains.

**Evidence.** The time grows with the work in an iteration, including work in called
functions, and does not change with the arena budget or after running. A deeper stack
gives a larger arena, and the argument budget adds to it exactly. A mutation that let
Keleusma's zero through made the test of the `None` case fail.

## Values, roadmap task 4

**Implemented on 2026-10-25.** Values convert by the entry point's declared types, in
`src/convert.rs`, with the types read in `src/types.rs`. A result is converted before
anything else runs on the VM, because in 0.2.2 a composite result lives in the arena and
is valid only until the next call or resume.

| Keleusma | Python out | Accepted in |
|---|---|---|
| `()` | `None` | `None` |
| `bool` | `bool` | `bool` only |
| `Byte` | `int` | `int` in 0 to 255, else `OverflowError` |
| `Word` | `int` | `int` in the signed 64-bit range, else `OverflowError` |
| `Fixed<N>` | `keleusma.Fixed` | Any exact number, else `ValueError` or `OverflowError` |
| `Float` | `float` | `float` or `int` |
| `Text` | `str` | `str` only |
| Tuple | `tuple` | A `tuple` of the declared length |
| Array | `list` | A `list` or `tuple` of the declared length |
| Struct | `dict` of its fields | Any mapping with exactly the declared fields |
| Enum | `keleusma.Variant(name, values)` | A `Variant`, or the name of a variant without fields |
| `Option<T>` | `None` or the value | `None` or a value of `T` |
| Newtype | Its underlying type | Its underlying type, except a refined newtype, which raises `TypeError` |
| `Multiword<N, F>` | `int`, or `Fraction` when `F` is not zero | Any exact number in range, else `OverflowError` or `ValueError` |
| A type the program does not define | The Python object passed in | Any Python object, carried as an opaque host value |
| Const-generic struct | As its instance | As its instance |
| Const-generic enum | As its instance | Not accepted, as described under Book coverage |
| Type-generic struct or enum, nested option | Not converted, `TypeError` naming the type | Not supported |

A `bool` is never accepted where a number is declared, although Python treats it as an
`int`, because passing `True` as a count is more often a mistake than an intent.

### Composite values

**Fact.** Keleusma 0.2.2 stores a tuple or array of scalars and text as a flat byte body
with no per-field types. `TupleBody::elements()` panics on that form, so the binding never
calls it. `ChunkSignature` records only the kind and byte size of a flat composite.

**Implemented.** `keleusma.compile` keeps the declared parameter and return types of
`main` from the parsed program. A flat body is read by those types, at offsets from
Keleusma's own `value_layout::LayoutDescriptor`. The compiler takes its layouts from the
same descriptor, through its type checker's `layout_for`, so the module documentation's
claim that no runtime path uses it is out of date. A body whose length differs from the
declared layout raises `keleusma.VMError` rather than being misread. A `Text` field is
read with Keleusma's `<String as KeleusmaType>::from_flat_bytes_ctx`, given an empty
opaque registry, which is sound because the converted types contain no opaque values.

**Evidence.** Round trips pass for every shape in the probe recorded for Dromon, through
an echo stream and through composites the script builds, which are the flat ones. A
mutation that read every field at offset zero crashed the interpreter, which is how the
defect below was found.

### Mistyped yields, a memory-safety guard

**Fact.** Keleusma 0.2.2's compiler and verifier accept a `yield` whose expression has a
different type from the declared return type. With a same-size mistype, for example
`yield (3, "wake")` where `(Text, Word)` is declared, Keleusma's safe `Vm::decode` reads
the integer 3 as a text pointer, and the process dies with a segmentation fault. This
was reproduced in plain Rust with no binding code, and reported to `v0.2.3` as
`pykeleusma-20261025-yield-soundness`.

**Implemented.** `src/yield_check.rs` repeats the compiler's type check,
monomorphisation and recording type check after compilation succeeds, then compares the
recorded type of every yielded expression with the declared return type, descending
into tuple and array literals whose type as a whole was not recorded. A mismatch is a
`keleusma.CompileError` at the yielded value. If the re-check itself fails, compilation
fails closed.

**Values with no recorded type.** Until 2026-10-25 an expression with no recorded type
was not compared, a residual risk. The guard now refuses one wherever the declared type
holds `Text` inside a composite, the only shape whose flat bytes hold a pointer. It
admits one case, a name bound by `let name = yield ...` in `main` when the resume type
equals the declared yield type, because the binding converted that value by exactly that
type. Elsewhere a mistyped value is caught or misread without a pointer when decoded.
Run over the test suite, the crate's scripts and the book's programs, the stricter guard
refused none that Keleusma runs correctly, and it refused a genuinely mistyped program
that Keleusma accepts, a resumed `Word` yielded as `(Text, Word)`. **What remains.** The
guard trusts the types Keleusma records. A wrong recorded type would be a Keleusma
type-checker defect, which PR 517 addresses for yields.

### Fixed-point values

**Decided on 2026-10-24 by the operator.** `Fixed` converts to a sane Python equivalent,
even if that equivalent carries metadata, and `float(value)` is the idiomatic way out.

**Fact.** A runtime `Value::Fixed(i64)` carries only the raw bits. The number of fraction
bits is part of the script's type, `Fixed<N>`, which the binding now reads from the
declaration. A bare `Fixed` takes the host target's default of 32 bits, as Keleusma's type
checker substitutes it. `N` ranges over 0 to 62, the parser's limits.

**Implemented.** `keleusma.Fixed` in `python/keleusma/_fixed.py` subclasses
`fractions.Fraction` and adds read-only `raw` and `frac_bits`. Equality, hashing,
ordering, `float()`, `int()`, `round()` and `math.floor()` therefore behave as they do for
the same rational. Arithmetic returns a plain `Fraction`, because fixed-width overflow
and rounding are Keleusma's semantics. `Fixed.from_value(value, frac_bits)` converts any
exact number and refuses rather than rounds, so a round trip is exact. Subclassing
`Fraction` replaced the earlier plan to register with `numbers.Rational`, which would
have required reimplementing the arithmetic.

## Book coverage, roadmap task 12

**Implemented on 2026-10-25,** at the operator's request that the binding cover the
functionality the Keleusma book describes. The ten example scripts of the 0.2.2 crate
run through the binding with their stated results, in `tests/test_book.py`. Of the 73
programs in the book, a first survey ran 41 without arguments. A second pass on
2026-10-25 ran the 14 that needed arguments, natives or signing, with suitable inputs.
Two of those needed binding work, opaque host values for the embedding guide's
`RustString` example and a typed `use` line for a native taking an array, which the
binding's message already asks for. The remaining 18 were fragments, programs the book
shows Keleusma rejecting, programs Keleusma's own compiler refuses, or the text
concatenation discrepancy. None failed because of the binding.

**Recoverable native errors.** A native raises `keleusma.NativeError(code, message)` to
report an error the script may handle with `host::f(x) { ok(v) => ..., error(code) =>
... }`, the construct of the book's chapters 33 and 34. The closure returns
`VmError::NativeErrorCode`. Any other exception is returned as a halting error, so the
construct never catches it. **Fact.** Keleusma's construct catches every error in its
soft host category, including `VmError::NativeError`, and gives it the code -1, so
returning that variant for every exception, as task 7 did, would have let a script
swallow a `KeyError` or a `KeyboardInterrupt`. The halting error is carried by
`InvalidBytecode`, because Keleusma has no halting variant for a host failure, and its
message is never shown, because the VM raises the stored exception instead. A stored
exception is raised only for an error the closure produced, so a native error that the
script handled earlier in the same run cannot mask a later runtime error.

**Recovery.** `vm.reset()` calls `Vm::reset_after_error`, which clears the operand
stack, the frames and the arena and keeps the data segment. It also abandons a
suspended stream.

**Libraries.** `libraries=` registers Keleusma's bundled `math`, `audio` and `shell`
natives, and `keleusma.library_signatures(name)` returns the declarations a script
includes. A host callable given for the same name takes precedence. The shell library
reaches the environment, processes and files, so a host offers it only by naming it.
Library natives are Rust code with Keleusma's own bounds, unlike Python natives.

**Precompiled bytecode.** `Module.to_bytes()` and `keleusma.load_module(data)`. The
wire format carries no source, so a loaded module converts by each value's own tag,
which admits scalars and text and refuses composites and `Fixed` with `TypeError`.
Without declared types the binding cannot refuse a mistyped argument before running,
so Keleusma's entry check refuses it as a runtime error and the VM fails until
`vm.reset()`. Keleusma 0.2.3 rejects the version 1 bytecode that 0.2.2 writes.

**Trust boundary for bytecode.** Bytecode is code, and the binding treats it as such.
The wire format's checksum detects corruption, not tampering. Keleusma's decoder and
verifier are the only checks on crafted bytes, and the binding has not established
that they are robust. **Fact.** Keleusma 0.2.2 can crash in its safe decode on a
program its own compiler accepts, which `v0.2.3` confirmed in
`v023-20261025-yield-soundness-reply`. **Inference.** A loaded module converts values
only by their own tags and refuses composites, which avoids the known crash route, but
that does not show the decoder is safe on crafted input. **Fuzzing.**
`tools/fuzz_load_module.py` loads, builds and runs mutated modules with a repaired
checksum. Its first 60,000 cases found one crash, a stack overflow in Keleusma's
`module_runtime_footprint`, which recursed without bound on a malformed control-flow
graph that Keleusma's verifier rejects. The binding sized the arena before Keleusma
verified the module, so it now runs the structural verifier first, and a following
run of 300,000 cases found no crash. That is evidence, not proof. Unsigned bytes
should therefore still come only from a source trusted as the script's source would
be. A signature establishes the producer and protects only as far as the holders of
the trusted keys are trusted.

**Library declarations.** Keleusma's compiler takes a script's `use math::sqrt(...)`
line on trust and does not compare it with the library's signature. A library native
builds its result by its own signature, while the binding converts by the script's,
so a mismatch reads a value as a type it does not have. A probe of `shell::run`
declared with its tuple reversed returned wrong values silently, and `v0.2.3` confirmed
that decoding with a mismatched type can crash 0.2.2. The binding therefore refuses a
module whose declaration of a library-served native differs from the library's, at
construction and at a swap. A native the host serves with a callable is not checked,
because the binding converts both directions by the script's declaration. A bare `use`
line converts by tag and is not checked.

**Signed and encrypted modules.** `Module.to_signed_bytes(seed, recipient_key=None)`
and `load_module(data, verifying_keys=..., decryption_key=...)`, the book's chapters 26
and 36. Keys are 32-byte strings, checked for length and, for verifying keys, for being
a valid curve point. The ephemeral key of an encryption comes from `os.urandom`. Giving
verifying keys for an unsigned module, or a decryption key for a plaintext one, is
refused, so a host that expects protected code cannot be handed unprotected code. A
module compiled from source declared `signed` runs only from signed bytes. **Trust
boundary.** The cryptography is Keleusma's, through `ed25519-dalek`, `x25519-dalek` and
`aes-gcm`. The binding implements no primitive. A signing seed or decryption key passed
from Python lives in a `bytes` object the binding cannot zero.

**Hot swap.** `vm.replace_module(module, private=None)` at an iteration boundary, after
`Reset` or `Finished`, the book's chapter 37. Shared data survives a swap with the same
layout, and Keleusma refuses a swap that changes it. Private data starts as `()`, the
state a fresh 0.2.2 VM starts in, unless `private` maps field names to values, which
Keleusma takes from the host at the swap. Only scalar fields and arrays of scalars can
be seeded, because Keleusma clears the composite body pool after writing the slots.
`vm.private_data` reads the running module's private fields, with `None` for one not yet
written, and passing its scalar fields as `private` carries them across a swap. Keleusma
has no accessor, so the binding reads the slots where Keleusma's own documentation of
`Vm::private_slot_count` places them, as consecutive values at the start of the
persistent region, between runs and read only. A Rust test pins that layout and runs
under Miri. **Fact.** A whole-array assignment to a private array stores the array in
the first element's slot and leaves the others `()`, so a later read of `pv.a[1]` gives
`()`, while element-wise stores work. `private_data` reads such an array whole, and
refuses it cleanly once a `Reset` has reclaimed it. Reported to `v0.2.3`. A native the new module adds must be
registered before the next call. Every native the host serves gets a fresh closure
with the new module's declaration, because a closure converts by the declaration of
the module it was built for, and a stale one built values of the old type until
2026-10-25. A swap also recovers a failed VM. Three facts about
Keleusma 0.2.2 shape the implementation, each verified with a pure Rust probe.

- `Vm::replace_module` verifies the new module's resource bounds against the arena's
  capacity, but without the registered natives' bounds and more loosely than `Vm::new`.
  A probe built a VM in a 256-byte arena, swapped in a module that `Vm::new` refuses for
  that arena, and the swap succeeded, after which the first call failed with
  `OutOfArena`. The binding applies its construction-time sizing rule, margin and native
  bounds included, to the new module first and refuses a module the arena is too small
  for, so a host builds the VM from the larger module.
- `Vm::replace_module_from_bytes` verifies the signature, then fails on every signed
  module with "FLAG_REQUIRES_SIGNATURE is set but the header carries no signature
  extension", because it keeps the signature flag on the decoded module and the swap
  re-encodes it without the signature. The binding verifies the bytes against the trust
  matrix itself and installs the decoded module with the flag cleared, the steps of
  Keleusma's own signed loader.
- `Vm::replace_module` ignores the trust matrix, so an unsigned module would replace
  signed code on a VM that trusts keys. The binding keeps its own copy of the matrix and
  refuses an unsigned swap once any key is trusted, the policy Keleusma's signed loader
  states. An encrypted module keeps its encrypted bytes, and Keleusma verifies their
  signature without the decryption key, so it swaps in under the same check.

**Opaque host values.** A type the program names but does not define is an opaque host
type to Keleusma's type checker, as the book's embedding guide describes. Any Python
object converts to one and the same object comes back, as a whole value, inside boxed
or flat composites, through yields and through natives. A flat composite stores an
opaque as an index into the VM's registry, which Keleusma keeps private. Natives
receive it in `NativeCtx`, and for a result the binding resolves the index through
`Vm::decode`, the only public path, as `compat::resolve_opaque` records. Each crossing
wraps the object anew, so a script comparing two crossings of one object sees two
values. An opaque value is unbounded Python memory, outside Keleusma's guarantees like a
Python native, and Keleusma counts it as zero arena bytes.

**Multiword big numbers.** `Multiword<N, F>` is `N` words, least significant first,
holding a signed two's complement integer, scaled by `2**F`, which a probe confirmed
by comparing a negative value with zero. It converts to an `int`, or to an exact
`Fraction` when `F` is not zero, by the exact-number rules of `keleusma.Fixed`.

**Typed shared data.** `vm.read_shared(buffer=None)` and `vm.write_shared(values,
buffer=None)` convert the shared block's fields by their declared types, in the VM's
own block or in a host buffer. Keleusma lays the block out as a packed struct of its
fields and records each slot's offset, one slot per element for an array field. The
binding compares every recorded offset with its own layout when a VM is built or
swapped and refuses typed access on any difference. A write converts every value before
changing a byte. Keleusma's own per-slot accessors handle only scalars, so the binding
reads with its flat decoder and writes with a flat encoder that passes only scalars to
`write_scalar_le`, which panics on anything else.

**Generic types.** A type-generic struct leaves the script as its substituted fields,
which probes confirmed for `Box<Word>` and `Pair<Word, Text>`, but cannot come in,
because the VM treats a host-built instance's generic field as opaque. A const-generic struct converts as its instance, whose name
monomorphization builds as `Buf__c3` for `Buf<3>`, which the binding reproduces so a
host-built value is recognised. A const-generic enum leaves the script but cannot come
in, because Keleusma 0.2.2 records only the generic definition's layout, without the
instance's padding, which a probe showed. A probe also found Keleusma mixing a flat `E__c2` and a
boxed `E` in one array, and rejecting another program after monomorphization, so
const-generic enum inputs stay refused. A type-generic enum such as `Maybe<Word>` failed
to type-check after monomorphization in Keleusma itself, and is refused.

**Not supported.** Type-generic enums, nested options, refined newtypes, type-generic
structs and const-generic enums as inputs, and composite private fields when seeding a
swap.

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

**Mistyped yields.** Described under Values. Keleusma 0.2.2's safe decode can segfault
on a program its compiler accepts, and the binding guards against the known form.

**Text concatenation.** Keleusma's FAQ states that a `fn main` doubling a text value
with `s + s` compiles and runs. In 0.2.2 it compiles, and the first `s + s` fails at run
time with `TypeError("cannot add KStr and KStr")`, verified with a pure Rust probe. This
is a discrepancy between Keleusma and its book, not a binding defect.

**Hot swap.** Three defects in `Vm::replace_module` and `Vm::replace_module_from_bytes`,
described under Book coverage, which the binding works around.

**Footprint analysis on unverified modules.** Keleusma 0.2.2's
`module_runtime_footprint` overflows the stack on a module whose control flow its
verifier rejects. The binding verifies first, as described under Book coverage.

**Generic layouts.** Keleusma 0.2.2 records only a const-generic enum's generic layout,
and lays out type-generic instances other than as their substituted fields, as
described under Book coverage.

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
- **Carrying composite private data across a hot swap,** which needs Keleusma to keep
  or expose the composite pool. Scalar private data already carries across through
  `vm.private_data` and `replace_module(module, private=...)`.
- **Publishing wheels.** `.github/workflows/wheels.yml` builds and smoke-tests wheels
  for Linux, macOS and Windows and the source distribution on every pull request. It
  publishes only from a version tag the operator pushes, after the operator approves
  the `pypi` environment, as `docs/process/RELEASING.md` describes.
