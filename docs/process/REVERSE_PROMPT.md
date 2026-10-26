# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-26, the pause until Keleusma 0.2.3

**Asked.** Check that the line is ready to be put on ice, act if needed, and record
the pause in `HANDOFF.md`.

**Checked, and ready.** The working tree is clean and `main` matches the remote. The
newest CI and Wheels runs on `main` and the `v0.2.2.0` tag run succeeded. There are no
open pull requests or issues, no open Dependabot or secret scanning alerts, and no
security advisories. The inbox is empty, and no sccache server is running. Code
scanning is not set up, so it has no alerts to report.

**Done.**
- `HANDOFF.md` opens with the pause, its restart trigger, what continues while
  paused, and what to do first on restart.
- Dependabot runs monthly instead of weekly, because each of its pull requests runs
  the full wheel matrix on the account's shared runners.
- Five local branches already merged into `main` were deleted with the safe delete.
  The sandbox kept git from removing their sections from `.git/config`, which is
  harmless.
- `HANDOFF.md` now says that only the sdist's provenance was checked, where it had
  claimed every file's.

**Unanswered.** Five requests to `v0.2.3` remain open, as `HANDOFF.md` lists. Replies
will wait in the inbox.
