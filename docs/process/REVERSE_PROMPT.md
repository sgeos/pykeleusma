# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-24, the operator's answers on task 1

**Done.** The design document now records three settled points. `keleusma.Fixed`
converts with `float(value)`, the Python idiom that `Fraction` and `Decimal` follow,
instead of a `to_float()` method. Native bounds use Keleusma's existing mechanism.
Natives and bounds are declared when the VM is built, the arena is sized with
`auto_arena_capacity_for`, and `verify_resources` re-checks any later change. The
threading model is one VM per dedicated thread, as the operator confirmed, and a new test
creates and runs VMs on four worker threads at once. `feat/owned-vm` was pushed and
merged into `main` through a pull request.

**Facts behind the native-bounds design.** The cookbook recipe "Auto-sizing the arena
from the module", read from the `v0.2.2` tag, passes per-native memory bounds to
`auto_arena_capacity_for` in `native_names` order. `examples/wcmu_attestation.rs` calls
`verify_resources` after `set_native_bounds`. Nothing inside `vm.rs` calls
`verify_resources`, so `call` and `resume` never re-check by themselves, and the
binding must.

**Verification.** `uv run pytest` passed 12 tests and `cargo test` passed 3. `ruff`,
`pyright` in strict mode, `cargo fmt --check` and `cargo clippy -D warnings` were clean.
Only macOS with Python 3.12.12 was checked.

**Sandbox proposal, not applied.** The key names come from the Claude Code settings
reference. The hostnames are inferred and should be confirmed against the sandbox's
denial messages.

- Project `.claude/settings.json` could allow writes to `~/.cache/uv` and `~/.cargo`,
  and the domains `index.crates.io`, `static.crates.io`, `crates.io`, `pypi.org`,
  `files.pythonhosted.org` and `github.com`, through `sandbox.filesystem.allowWrite`
  and `sandbox.network.allowedDomains`.
- The interline store is outside this repository. An allowance for its directory, not
  only its database file, belongs in user settings or an ignored local settings file.
- The documentation states that SSH `git push` fails inside the macOS sandbox even for an
  allowed host. The options are an HTTPS remote with a token, or a narrow
  `sandbox.excludedCommands` entry such as `git push *` in user settings.

**Next.** Task 2, `keleusma.compile` with located exceptions.
