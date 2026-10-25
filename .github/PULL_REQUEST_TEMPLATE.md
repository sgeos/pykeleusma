<!--
Replace this comment with a short summary of what the change does and why. Reference a
related issue with `Fixes #N` or `Closes #N`.
-->

## Summary

<!-- One or two sentences. -->

## Pre-merge checklist

- [ ] `uv run pytest` passes, with the expected test count
- [ ] `uv run ruff check . && uv run ruff format --check .` and `uv run pyright` are clean
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` are clean
- [ ] `python/keleusma/_keleusma.pyi` matches any change to `src/lib.rs` or the classes it registers
- [ ] `CHANGELOG.md` is updated under `[Unreleased]` if the change is user-visible
- [ ] Nothing implies that a Python native or a Python object carried as an opaque value is bounded
- [ ] The pull request is labelled `full-ci` if it is a release or a platform-specific change
