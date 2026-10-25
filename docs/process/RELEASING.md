# Releasing

> **Navigation**: [Process](./README.md) | [Documentation Root](../README.md)

Publishing to PyPI is irreversible. PyPI never accepts a file name twice, so a release
that is deleted cannot be uploaded again under the same version. Every step that
publishes is the operator's, and an agent prepares the release only up to the tag.

## One-time setup, by the operator

1. **Trusted publisher on PyPI.** Under the operator's PyPI account, add a pending
   trusted publisher for the project `pykeleusma`, with owner `sgeos`, repository
   `pykeleusma`, workflow `wheels.yml` and environment `pypi`. No token is stored in the
   repository.
2. **The `pypi` environment on GitHub.** Under the repository's settings, create the
   environment `pypi`, add the operator as a required reviewer, and restrict its
   deployments to tags matching `v*`. Without the environment, GitHub would create it
   on the first tag with no protection, and the publish job would run unapproved.
3. **Private vulnerability reporting.** Enable it under the repository's security
   settings, because `SECURITY.md` directs reports there.

## Each release

1. **Prepare on a branch.** Set the version in `pyproject.toml`, and in `Cargo.toml`
   too when the Keleusma release changes. Run
   `uv run python tools/third_party_licenses.py` if `Cargo.lock` changed. Move the
   `Unreleased` section of `CHANGELOG.md` under a heading with the version and the
   date. Update the status statements in `README.md` and `CLAUDE.md`, including the
   sentence that nothing is published yet before the first release. Merge through a
   pull request once CI passes with full counts.
2. **Tag the merge commit.** `git tag -a v0.2.2.0 -m "pykeleusma 0.2.2.0"` on `main`,
   then `git push origin v0.2.2.0`. The tag must be the version with a leading `v`,
   because the publish job refuses files whose names do not match it.
3. **Approve the deployment.** The wheels workflow builds and smoke-tests every
   distribution again, then waits for approval of the `pypi` environment. Approving it
   uploads the 24 wheels and the source distribution.
4. **Check the result.** In a clean environment, `pip install pykeleusma==0.2.2.0` and
   run a script, then confirm the project page renders the README.

A rehearsal against TestPyPI needs its own trusted publisher and a variant of the
publish job, and is not set up.
