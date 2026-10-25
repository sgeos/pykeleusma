# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, GitHub configuration at parity, and README badges

**Asked.** Configure the GitHub repository sensibly, at parity with the Keleusma and
angelssword-adventurers-creator repositories where that makes sense, with badges in
the README.

**Done, repository settings.** Applied through the GitHub interface, not tracked.
- Topics, and a description naming the bounds. Merged branches are deleted. The unused
  wiki and projects are off. All three repositories match on these, and on merge
  methods, Actions permissions and secret scanning, which were already alike.
- Private vulnerability reporting and Dependabot vulnerability alerts are on, as in
  both references. Dependabot security updates stay off, as in both.
- A ruleset named Protect main, copied from angelssword-adventurers-creator, blocks
  force-pushes to and deletion of `main`.
- Five merged feature branches left on the remote were deleted, after checking that
  each is contained in `main`.

**Done, files.** Badges for CI, wheels, PyPI, the Python floor, the Keleusma pin, the
license and the status, with a test for the static ones. Issue templates, a contact
link configuration and a pull request checklist, modelled on Keleusma's and smaller.

**Not copied, deliberately.**
- Keleusma's required linear history, which would forbid this repository's merge
  commits.
- Required status checks, which would leave a pull request blocked whenever path
  filters skip a workflow.
- GitHub Pages, which Keleusma uses for its book. This binding's documentation lives in
  the repository.
- Required sign-off on web commits, which angelssword has and Keleusma does not.

**Flagged.** The PyPI badge reads "package not found" until the first release. The
homepage field still points at the Keleusma repository, and could point at the PyPI
page after release, which is the operator's choice.

**Verification.** 411 Python tests pass locally, including the new badge test, which a
stale badge was shown to fail. Pull request 25's full run under `full-ci` passed every
job, FreeBSD with 409 tests and 1 skip. This pull request is the first to exercise the
reduced set and the quick wheel.
