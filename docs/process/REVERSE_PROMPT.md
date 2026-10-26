# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-26, the publication of 0.2.2.0

**Asked.** Double check that the deployment is complete.

**Verified.**
- The tag run of `wheels.yml` for `v0.2.2.0` on `aa383b7` succeeded, and its publish
  job finished at 21:14:57 in Coordinated Universal Time after the operator's approval.
- PyPI lists version 0.2.2.0 with 25 files, the 24 wheels and the source distribution,
  none yanked, with license `0BSD` and `Requires-Python >=3.12`.
- The source distribution's provenance attestation names the publisher `GitHub`,
  repository `sgeos/pykeleusma`, workflow `wheels.yml` and environment `pypi`. Only
  that file's attestation was fetched.
- `pip install pykeleusma==0.2.2.0` from PyPI into a clean environment on macOS ARM64
  with CPython 3.14 chose the stable-ABI wheel, and a compiled script returned
  `Finished(42)`. `keleusma.KELEUSMA_VERSION` reported 0.2.2 and the bytecode version 1.
- The project description is the README as Markdown, without the old sentence that
  nothing is published, and the PyPI badge reports v0.2.2.0.

**Not verified.** Installation on other platforms from PyPI. The same wheels were
smoke-tested in CI before upload, but not downloaded back.

**Before this.** PR #29 from Dependabot updated `setup-uv` and `upload-artifact`, and
was merged before the release at the operator's direction. PR #30 prepared the
release and passed the full CI with those updates.

**Optional for the operator.** Point the repository's homepage field at the PyPI page,
and create a GitHub release from the tag.
