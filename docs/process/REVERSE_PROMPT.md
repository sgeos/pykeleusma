# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, library declaration check, bytecode trust note, no co-author trailer

**Goal.** Add the library signature check and the bytecode trust note, remove the
co-authored note, and stay on Keleusma 0.2.2.

**Library declarations.** A module whose declaration of a library-served native
differs from the library's signature is refused, at construction and at a swap. A host
callable for the native and a bare `use` line are not checked, for reasons the design
records. The published signatures of all three libraries pass the check.

**A further binding defect, found while testing and fixed.** After a hot swap, a native
served by a host callable kept converting by the first module's declaration. Every
swap now rebuilds those closures. Both changes have tests that fail without them.

**Bytecode trust.** The README, the design, the `load_module` docstring and the stub
state that bytecode is code, that the checksum is not authentication, that Keleusma's
decoder is not known to be robust against crafted bytes, and that a signature
identifies a producer rather than making bytes safe. No fuzzing was done.

**Co-author trailer.** Removed from `CLAUDE.md` and omitted from this commit. Earlier
commits keep theirs, because removing them would need a force-push.

**Keleusma version.** Staying on 0.2.2, recorded in `HANDOFF.md`.

**Verification.** 334 Python tests and 30 Rust tests pass locally, with pyright, ruff,
rustfmt and clippy clean.
