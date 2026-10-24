# Standalone Repository

> **Navigation**: [Decisions](./README.md) | [Documentation Root](../README.md)

**Decided 2026-10-24 by the operator**, who asked for a standalone repository named by
Python convention, targeting the published Keleusma 0.2.2, minimal at first. This record
keeps the analysis the decision rested on. The analysis was written by the Keleusma
project's kaizen line, which cut this repository.

## Question

Should the Python binding live in its own repository, or inside the Keleusma repository
as many Rust projects keep their bindings?

## Reasons for a separate repository

- **A different toolchain.** The binding needs PyO3, maturin, Python packaging, PyPI and a
  wheel matrix across operating systems. None of that belongs in Keleusma's gates. Its
  continuous integration took about 31 minutes on 2026-10-24 and is guarded by tests that
  check its gate and workflow against each other.
- **A different pace.** The binding depends on `keleusma = "=0.2.2"` from crates.io and
  releases on its own schedule. A binding inside the Keleusma repository would follow the
  version branch as it changes, which is the opposite of targeting a published release.

## The cost

Drift. When Keleusma 0.2.3 is published, the binding must move deliberately. That move
changes the bytecode version from 1 to 2, so it is visible to users and is better made as
a conscious release than absorbed silently. For a minimal first version this trade is
acceptable.

## Name

The repository and distribution are `pykeleusma`, and the import package is `keleusma`.
This follows wrappers of native libraries such as `pygit2`, `pyzmq` imported as `zmq`,
and `pycairo` imported as `cairo`. On 2026-10-24 PyPI returned 404 for both `pykeleusma`
and `keleusma`. That suggests the names are free, and it does not prove it, because PyPI
can reserve or refuse a name.

## Scope of 0.1.0

Minimal, as the operator allowed. Compile source, verify, call and resume, register
Python callables as natives, and convert scalars, strings, tuples and arrays. The
milestone is [`../roadmap/V0_1_0.md`](../roadmap/V0_1_0.md) and the design is
[`../architecture/BINDING_DESIGN.md`](../architecture/BINDING_DESIGN.md).
