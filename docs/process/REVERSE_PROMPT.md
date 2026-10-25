# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, a FreeBSD job and a lighter CI load

**Asked.** The operator asked for a FreeBSD job, then approved this line's proposal
to the admin session's CI load review, recommendation included.

**Done.**
- **FreeBSD.** `ci.yml` builds the source distribution, installs it in a FreeBSD 15.1
  virtual machine through `vmactions/freebsd-vm`, pinned to its v1.5.9 commit, and runs
  the tests there. PyPI accepts no FreeBSD wheels, so this is a FreeBSD user's path.
- **Load.** A pull request runs Linux 3.12 and 3.15, Windows 3.12, 3.14t and Miri, and
  a `plan` job that chooses one quick Linux wheel, or the full 24 when packaging paths
  change. A push to `main`, a manual run and the `full-ci` label run everything. The CI
  test matrix is capped at 3 parallel jobs and the wheel builds at 4. Documentation
  alone runs nothing, except `README.md`, which tests check. A typical source-only pull
  request goes from 21 jobs to 7. The publish job, its environment, its tag check and
  its file count are unchanged, and GitHub's documentation confirms that path filters
  are not evaluated for tag pushes.

**Deviation from the proposal.** The proposal counted 6 jobs. Making `wheels.yml`
reusable would have let a caller without `id-token: write` fail the publish job's
validation, so the choice moved into a small `plan` job, the seventh.

**Changed rule.** `main` is kept green rather than always green. A failure on `main`
after a merge is fixed before other work, and `CLAUDE.md` records this.

**Verification.** actionlint passes. The plan job's path rule was tested locally on
sample file lists. The first full run of the new workflows is this pull request's,
under the `full-ci` label.
