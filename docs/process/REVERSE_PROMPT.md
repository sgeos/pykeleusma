# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, more wheels, and the operator's answers on publication

**The operator's answers.** Items 1, 2, 6 and 7 of the last report wait for publication.
Item 3 was a question, answered in session. Item 4, the license expression stays
`0BSD`, with the bundled crates covered by `THIRD_PARTY_LICENSES.txt`. Item 5, this is a
best-effort adapter for Keleusma 0.2.2, and wheels are wanted for musl Linux, Windows on
ARM64 and free-threaded Python.

**Done.**
- **24 wheels.** Eight platform builds, glibc and musl Linux, macOS and Windows, each on
  x86-64 and ARM64. Each build gives a stable-ABI wheel for CPython 3.12 and later and
  one wheel each for free-threaded 3.14t and 3.15t, which the stable ABI does not cover.
  Musl wheels are smoke-tested in an Alpine container. The free-threaded musl wheels
  are only checked for their notices, because no official free-threaded Alpine image
  exists.
- **Free-threading review.** PyO3 0.30 already declares the module safe without the
  GIL, so the review in the design's Threading section is what that rests on. A new
  test runs eight threads through the shared state at once, which passed 50 runs under
  a local 3.14t, a smoke test fails if the GIL comes back on, and a CI job runs the
  suite on 3.14t. pygame-ce has no free-threaded build, so the frame-loop test skips
  there.
- **Classifiers** are tested against PyPI's list, including the new free-threading one,
  `2 - Beta`.

**Needs the operator.** Only the items already deferred to publication, listed in
`docs/process/RELEASING.md`. Private vulnerability reporting can be enabled in the
repository settings, under Advanced Security, or with
`gh api -X PUT repos/sgeos/pykeleusma/private-vulnerability-reporting`.

**Verification.** 409 Python tests and 49 Rust tests pass locally. Under a local
free-threaded 3.14t build of the final code, 408 pass and the frame-loop test skips
for want of pygame. The Windows ARM64 and musl
builds were not run locally, and CI is their first test.
