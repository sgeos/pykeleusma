# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-24, the sandbox check

**Result.** With the operator's user-level allowances, the whole development loop runs
inside the sandbox once two local tools are dealt with. With uv 0.13.5 and sccache
bypassed, `uv sync`, 12 Python tests, 3 Rust tests, `ruff`, `pyright`, `cargo fmt`,
`cargo clippy` and Miri under Tree Borrows all passed in the sandbox. Bare `git fetch`
works through the operator's exclusion. Reading the interline store works and writing
does not.

**Proposed user-level changes, not applied.**
1. Upgrade uv. The installed 0.9.5 panics in the sandbox, and 0.13.5 does not.
2. Set `RUSTC_WRAPPER` to an empty value for agent sessions, through the `env` key of the
   user settings, so that sandboxed builds skip sccache. Terminal builds keep it.
3. Add the interline store's directory to the user-level `sandbox.filesystem.allowWrite`.

**Security note.** A trace of sccache printed the process environment, including API
keys exported by the operator's shell, into the session transcript. The keys are not in
any tracked file. Rotating them is advisable. Every sandboxed command inherits those
variables, which is worth reviewing separately.

**Mailbox.** `v0.2.3` replied that the Miri report is known, and that Keleusma runs Miri
under Tree Borrows for the same reason. The design document records the reply, and the
message is acknowledged.

**Next.** Task 2, `keleusma.compile` with located exceptions.
