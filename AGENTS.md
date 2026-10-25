# AGENTS.md

Guidance for AI coding assistants other than Claude Code, such as Codex.

## Authoritative source

The project context, conventions and per-session protocol live in
[`CLAUDE.md`](./CLAUDE.md). Despite the name, its content is agent-agnostic. Read it before
making any non-trivial change.

## Reading order

1. [`CLAUDE.md`](./CLAUDE.md).
2. [`docs/process/HANDOFF.md`](./docs/process/HANDOFF.md), the state of the work and its
   validity checks.
3. [`docs/process/REVERSE_PROMPT.md`](./docs/process/REVERSE_PROMPT.md), the last report.
4. [`docs/roadmap/V0_1_0.md`](./docs/roadmap/V0_1_0.md) and
   [`docs/architecture/BINDING_DESIGN.md`](./docs/architecture/BINDING_DESIGN.md).

## Flagged up front

- **HARD CONSTRAINT: before writing anything tracked, read
  `~/projects/rust/keleusma/secret/notes/APPENDIX_B.md`.** Do not copy any of it into
  tracked files, commit messages, or public documentation. Reference it only as
  "Appendix B".
- **Keleusma is a pinned dependency, not a sibling.** Use `keleusma = "=0.2.2"` from
  crates.io. Do not edit the Keleusma repository.
- **Commit once per prompt,** and attribute the assistant that actually contributed.
- **Ask before outward-facing actions,** such as creating a remote, pushing, or publishing.
- **Scratch directories.** Use `tmp/` for transient files. Its contents are ignored. Never
  commit anything under `secret/`.
