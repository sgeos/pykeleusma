# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, readiness for publication, roadmap task 15

**Goal.** The project well documented, its technical debt paid, its limitations
prominent with a note that the Keleusma-based ones are expected to clear with the
public 0.2.3, every issue reported to Keleusma, and the project in order for
publication.

**Done.**
- **Limitations.** A section of the README, placed before the guarantees. It lists the
  limitations from Keleusma 0.2.2 under the statement that they are expected to clear
  with the public 0.2.3, says that some are fixed on Keleusma's branch and the rest
  unconfirmed, and lists the binding's own design limits apart. It also warns that
  bytecode saved now will not load after the move to 0.2.3. Tests pin these
  statements and the section's position.
- **Reporting.** Every Keleusma issue in the design was compared with the mailbox. One
  was unreported, that the compiler takes a library native's `use` line on trust. A
  pure Rust probe reproduced it, and it was sent as
  `pykeleusma-20261025-library-declarations`, with an index of every open item and a
  question about which are not expected to clear in 0.2.3. The design's Known issues
  section now records the reporting.
- **Debt.** Four workarounds lacked the `KELEUSMA-0.2.2` marker, the library
  declaration check, the regrowth margin, the trampoline pool and scalar-only seeding,
  and now carry it. Rustdoc gave eight warnings, from a stale crate comment and two
  links to test-only methods, and gives none. CI now fails on a rustdoc warning. The
  package docstring no longer calls the interface planned.

**Inference, flagged.** The README's statement that the Keleusma-based limitations will
clear with 0.2.3 rests on the operator's expectation. It is not verified item by item,
and the README says so.

**Needs the operator.** Only publication, as `docs/process/RELEASING.md` lists.

**Verification.** 410 Python tests and 49 Rust tests pass locally, with ruff, pyright,
clippy, rustfmt and rustdoc clean. The source distribution passes `twine check
--strict`. The 24 wheels of the last wheels run were checked earlier the same day with
`twine check --strict`, for their metadata and license files.
