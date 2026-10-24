# Binding Design

> **Navigation**: [Architecture](./README.md) | [Documentation Root](../README.md)

The planned shape of the binding, written when the repository was cut on 2026-10-24.
Every Keleusma API named here was read from the `v0.2.2` tag of the Keleusma repository.
Nothing here is implemented yet, so each section is a plan, and the risks are stated
where the plan rests on an assumption.

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

- **Planned.** A self-referential owner built with the `self_cell` crate, which owns the
  `Arena` and the `Vm` that borrows it, and drops them in the right order. Version 1.3.0
  was current on 2026-10-24.
- **Rejected.** `Box::leak` of the arena. It is simple, but it leaks one arena for every
  VM a Python program creates, which a long PyGame session would do repeatedly.

This is the first task of the milestone, because everything else depends on it. A spike
that constructs, calls and drops a VM inside a `#[pyclass]` settles it.

## Risk 2, Python natives are not bounded

A native registered with `register_native_closure` receives the default bounds,
`DEFAULT_NATIVE_WCET` of 10 cost units and a default memory figure, and
`Vm::set_native_bounds` can change them. A Python callable has no bound at all, since it
can run for any time and allocate any memory.

The binding must state plainly that Keleusma's worst-case guarantees cover the script and
the bounds declared for each native, never the Python code behind a native. Whether to
expose `set_native_bounds` in 0.1.0 is open.

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
| `Fixed` | Open. A small `Fixed` class, or a `float` with a stated precision loss |
| Structs and enums | Open, possibly dictionaries or generated classes, after 0.1.0 |

## Threading

`Vm` is not documented as `Send`, so the class should be `#[pyclass(unsendable)]`, which
makes PyO3 raise if another thread uses it. A native closure must take the GIL before it
calls Python. Free-threaded Python is out of scope for 0.1.0.

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
