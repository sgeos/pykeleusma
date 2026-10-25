# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, shared data kept in Rust

**Instruction.** The shared memory is kept in the Rust code. It syncs from Python, calls
the VM, and then flushes to Python. Anything Python does in the meantime is a matter for
the Python code, not for Keleusma.

**Done.** The VM owns its shared `data` block as a Rust vector for its whole life,
zeroed at construction, and lends it to Keleusma on every call. A call given `shared=`
syncs the block from that Python buffer, runs, and flushes the block back. Python's
memory is never lent to Keleusma.

- **Changed.** A call without `shared=` used to raise `TypeError` for a module with a
  block. It now runs on the VM's own copy, so state persists across calls even without
  a Python buffer. `vm.shared_data` returns a copy of the block.
- **Kept.** After a failed call the flush is skipped, so the Python buffer keeps its
  pre-call contents, and the VM is failed. Flushing after a failure as well is a one-line
  change if preferred.

**Verification.** 212 Python tests and 27 Rust tests passed locally. New tests cover
persistence without a buffer, syncing over the VM's copy, and a native's write to the
Python buffer being overwritten by the flush. A mutation that cleared the Rust copy
between calls made the persistence test fail. `ruff`, `pyright`, `cargo fmt` and
`cargo clippy` were clean.

**Merged.** Through pull request 13, after CI passed on Linux and macOS with Python 3.12
and 3.15 and under Miri. `pydromon` was told about the changed semantics in
`pykeleusma-20261025-shared-data-update`.

**Standing permission.** The operator granted standing permission on 2026-10-25 to
commit, push and merge work through a pull request without asking each time. Publishing
to PyPI, creating a remote and force-pushing still need confirmation in session.
