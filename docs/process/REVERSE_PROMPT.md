# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, the readiness check before publication

**Asked.** Double check that the GitHub repository is configured correctly, that all CI
is green, and that the project is otherwise ready to publish.

**Checked, and as expected.**
- The repository settings, topics, ruleset, private vulnerability reporting and
  Dependabot alerts match what was set. There are no open Dependabot or secret
  scanning alerts, pull requests, issues, tags or releases.
- The newest CI and Wheels runs on `main`, at `13255fd`, passed every job. Regular
  jobs ran 411 Python and 49 Rust tests, 3.14t and FreeBSD ran 410 with 1 skip, and
  Miri ran 44 with 5 ignored. All 24 wheels and the source distribution built.
- The local gate passes on `main`. The source distribution passes `twine check
  --strict` and holds no scratch, secret or agent files.
- `pykeleusma` is unregistered on PyPI and on TestPyPI.
- The publish job's tag condition, environment, identity-token permission and file
  count are unchanged.

**Fixed.** The protected `pypi` environment did not exist, so a pushed tag would have
created it unprotected. It now exists, with the operator as required reviewer,
self-review allowed so that the operator can approve their own release, and
deployments restricted to `v*` tags. `RELEASING.md` records it as done.

**Needs the operator.** Registering the trusted publisher on PyPI, choosing the release
date for the release pull request, and pushing the tag and approving the deployment.
