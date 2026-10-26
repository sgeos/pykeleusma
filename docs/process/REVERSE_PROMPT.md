# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-26, the release pull request for 0.2.2.0

**Asked.** Prepare the release pull request with today's date. The operator is in
Pacific time, where the date is 2026-10-26, the same as in Coordinated Universal Time
when the change was made.

**Changed.**
- `CHANGELOG.md` moves the unreleased entries under `[0.2.2.0] - 2026-10-26`, keeps an
  empty `[Unreleased]` section, and adds link references for both.
- `README.md` replaces the sentence that nothing is published with one naming 0.2.2.0
  as the first release on PyPI. The status stays early development.
- `CLAUDE.md` states the release and that publication follows the tag and approval.
- `CITATION.cff` gains `version` and `date-released`.
- The version in `pyproject.toml` and `Cargo.toml` already reads 0.2.2.0 and 0.2.2.

**Caveat.** The README and CLAUDE.md describe the release as published once this
merges, which becomes true only when the operator pushes the tag and approves the
deployment. If publication is delayed, these statements overstate the state until it
happens.

**Needs the operator.** Register the PyPI trusted publisher, push `v0.2.2.0` on the
merge commit, and approve the `pypi` deployment.
