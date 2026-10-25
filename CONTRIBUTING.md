# Contributing

Contributions are welcome, including fixes, documentation and tests.

**The project is licensed under 0BSD.** By contributing you agree that your contribution
is released under the same licence. Do not contribute material that you did not write,
or that carries terms of its own.

Participation in the project's spaces follows the
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md). Contributions are judged on their technical
merits.

## Where a change belongs

This repository is the Python binding. The language, its compiler and its virtual
machine are [Keleusma](https://github.com/sgeos/keleusma), which the binding uses as a
published release pinned exactly in `Cargo.toml`. A defect or request that concerns
Keleusma itself belongs in that repository. If the boundary is unclear, open the issue
here, and it will be forwarded.

Keleusma's worst-case bounds cover the script and the bounds declared for each native.
A Python callable registered as a native, and a Python object carried as an opaque
value, have no bound. A change must never imply otherwise, in code, documentation or
tests.

## Before opening a pull request

1. Run `./scripts/check.sh` and confirm that it exits zero. It needs
   [uv](https://docs.astral.sh/uv/), a stable Rust toolchain, `python3` and
   `shellcheck`, and it builds the extension before testing it.
2. After a change to `src/lib.rs` or to the classes it registers, update
   `python/keleusma/_keleusma.pyi` to match, because the type checker reads the stub.
3. Keep each pull request to one change, on a branch named
   `<scope>/<short-description>`, such as `fix/native-error-position`.
4. Write commit messages as conventional commits, such as `feat:`, `fix:`, `docs:`,
   `test:` or `chore:`.
5. Add an entry under `[Unreleased]` in `CHANGELOG.md` for any change a user would notice.
6. Do not weaken a check to make it pass. If a check is wrong, say so in the pull request
   and fix it deliberately.

A pull request runs a reduced set of continuous integration jobs, and the full set runs
after the merge to `main`. The maintainer may label a pull request `full-ci` to run the
full set first, as for a release or a platform-specific change.

## Reporting security issues

Do not open a public issue for a vulnerability. Follow [`SECURITY.md`](SECURITY.md).
