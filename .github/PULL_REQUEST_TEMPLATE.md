<!--
Replace this comment with a short summary of what the change does and why. Reference a
related issue with `Fixes #N` or `Closes #N`.
-->

## Summary

<!-- One or two sentences. -->

## Type of change

- [ ] Feature
- [ ] Fix
- [ ] Documentation
- [ ] Tests
- [ ] Maintenance, such as continuous integration, packaging or process

## Pre-merge checklist

- [ ] `./scripts/check.sh` exits zero, with the expected test counts
- [ ] `python/keleusma/_keleusma.pyi` matches any change to `src/lib.rs` or the classes it registers
- [ ] `CHANGELOG.md` is updated under `[Unreleased]` if the change is user-visible
- [ ] Nothing implies that a Python native or a Python object carried as an opaque value is bounded
- [ ] Any figure quoted in the change was measured on this tree, or the same sentence says it was not
- [ ] No check was weakened to make this pass. If a check is wrong, the change says so and fixes it deliberately
- [ ] Nothing from `secret/` or `tmp/` is included
- [ ] The pull request is labelled `full-ci` if it is a release or a platform-specific change

## Notes for the reviewer

<!-- Alternatives considered, places where a second opinion would help, or anything unverified. -->
