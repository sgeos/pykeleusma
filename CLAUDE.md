# CLAUDE.md

Guidance for Claude Code and, through `AGENTS.md`, for other agents working in this
repository. It is loaded into every session and re-read after compaction, so it holds
only what applies broadly. History lives in git and in `CHANGELOG.md`.

## Project Overview

pykeleusma is the Python binding for [Keleusma](https://github.com/sgeos/keleusma), a total
functional stream processor whose programs have statically bounded worst-case execution
time and memory use. The goal is to run Keleusma scripts inside Python programs, such as a
PyGame frame loop or a NumPy computation. The distribution is `pykeleusma` and the import
package is `keleusma`.

**Status.** Cut on 2026-10-24. Version 0.2.2.0 builds against the published Keleusma 0.2.2,
and tasks 1 to 15 of the first milestone are done. The binding covers what the Keleusma
book describes, except what is blocked on Keleusma. CI tests Linux, macOS and Windows,
and wheels are built but not published.
Publishing is prepared and waits on the operator, as
[`docs/process/RELEASING.md`](docs/process/RELEASING.md) describes. The milestone is
[`docs/roadmap/V0_1_0.md`](docs/roadmap/V0_1_0.md), the design is
[`docs/architecture/BINDING_DESIGN.md`](docs/architecture/BINDING_DESIGN.md), and the move to
0.2.3 is planned in
[`docs/architecture/MIGRATION_V0_2_3.md`](docs/architecture/MIGRATION_V0_2_3.md).

**Relationship to Keleusma.** This repository depends only on the crates.io release, pinned
exactly as `keleusma = "=0.2.2"`. Never build against a Keleusma checkout and never edit
the Keleusma repository. The binding's version is that release with a fourth part counting binding releases, held in
`pyproject.toml` because Cargo admits only three parts. Moving to a later release is a deliberate change, because
Keleusma 0.2.3 changes `BYTECODE_VERSION` from 1 to 2. Calls that 0.2.3 changes live in
`src/compat.rs`, and every 0.2.2 workaround is marked `KELEUSMA-0.2.2`. Questions about
Keleusma go to the Keleusma lines through the mailbox described in
`docs/process/HANDOFF.md`.

**The guarantee boundary.** Keleusma's worst-case bounds cover the script and the bounds
declared for each native. A Python callable registered as a native has no bound. The
binding must never imply otherwise.

## Development Process

**HARD CONSTRAINT, before any of the below.** Before writing anything tracked, read
`~/projects/rust/keleusma/secret/notes/APPENDIX_B.md`. It holds operator-stated context held
outside the repository deliberately. **Do not copy any of it into tracked files, commit
messages, or public documentation; reference it only as "Appendix B".** The failure mode is
disclosure, which cannot be undone.

**Session startup protocol.**
1. Read [`docs/process/HANDOFF.md`](docs/process/HANDOFF.md) and run its validity checks.
   Report the handoff as valid, or as stale, on their outcome.
2. Check the `pykeleusma` inbox as `HANDOFF.md` describes.
3. Read [`docs/process/REVERSE_PROMPT.md`](docs/process/REVERSE_PROMPT.md) for the last report.
4. Wait for a human prompt before proceeding.

**After completing each task.**
1. Mark the task in the roadmap table.
2. Overwrite `docs/process/HANDOFF.md` and `docs/process/REVERSE_PROMPT.md`. Both are state
   files. A rewrite that drops an open item names it in the commit message as completed,
   moved or abandoned.
3. Commit once per prompt with a conventional commit.
4. If blocked or uncertain, say so in `REVERSE_PROMPT.md` and **stop**.

`docs/process/PROMPT.md` is read-only for the agent, and is included in the commit if the
operator has modified it.

## Git Workflow

`main` is kept green. Work happens on short-lived feature branches merged into `main` by
a no-fast-forward merge once checks pass, through a pull request once a remote exists.
GitHub runners are shared across the operator's account, so since 2026-10-25, at the
operator's approval, a pull request runs a reduced set of jobs and a push to `main` runs
the full set, as the headers of `ci.yml` and `wheels.yml` describe. A failure on
`main` after a merge is fixed before any other work. Label a pull request `full-ci` to
run the full set before merging, as for a release or a platform-specific change. Use
scoped conventional commits such as `feat:`, `fix:`, `docs:`, `test:` and `chore:`.
Commit messages carry no `Co-Authored-By` trailer, at the operator's instruction of
2026-10-25. Pull requests merge with a merge commit only, because squash and rebase
merging are disabled in the repository settings.

**One writer per checkout.** When more than one line works on the repository at once,
each works in its own worktree on its own branch, created with `scripts/worktree.sh`.
Two lines in one checkout share an index and a dirty tree, so one line's `git add`,
stash or reset can take in or discard the other's work.

**Irreversible or outward-facing actions need the operator's confirmation in session.**
That includes creating or pushing to a remote, publishing to PyPI or crates.io, and
force-pushing.

## Common Commands

```sh
./scripts/check.sh                            # the whole local gate below, apart from Miri
./scripts/check.sh --hygiene-only             # repository hygiene only, as CI's repository job
scripts/worktree.sh new feat/some-thing       # an isolated worktree for a parallel line
uv sync                                       # build the extension and install dev tools
uv run pytest                                 # tests
uv run ruff check . && uv run ruff format --check .
uv run pyright                                # strict type checks
cargo fmt --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items
cargo test                                    # Rust unit tests
MIRIFLAGS=-Zmiri-tree-borrows CARGO_TARGET_DIR=target/miri cargo +nightly miri test --lib
```

`uv sync` rebuilds the extension when Rust sources change, through the `cache-keys` in
`pyproject.toml`. Miri runs under Tree Borrows because of the dependency issue recorded
in the design document. After editing `src/lib.rs`, keep
`python/keleusma/_keleusma.pyi` in step, because pyright reads the stub, not the binary.
`scripts/check.sh` needs `shellcheck`, and its hygiene section is kept identical to
`sgeos/project-template` so that a later sync is a plain comparison. If
`secret/disclosure-terms.txt` exists, one term per line, the gate fails when a tracked
file contains any of them and prints only the file and line. The scan cannot run in
continuous integration, which does not have the list.

## Before Trusting a Green Run

- **Know what the command covered.** A test run that imported a stale build of the
  extension tested the old code. Rebuild, then test.
- **Count what was reached.** A suite that collected zero tests, or skipped them all, passes.
  Check the count in the summary line.
- **A local pass is not a remote pass.** Continuous integration runs on other operating
  systems and tool versions, and a pull request runs only the reduced set. Say which
  one was observed.

## Scratch and Secret Directories

`tmp/` is project-local scratch for drafts, commit messages, probe outputs and scratch
scripts. Its contents are ignored and only `tmp/.gitkeep` is tracked. `secret/` holds
local material that is never committed, such as credentials, and is ignored whole. Both
follow the Keleusma repository.

## Coding Conventions

- **Rust.** Edition 2024. Every fallible path returns a `PyResult`, and no panic crosses into
  Python, with the single recorded exception of PyO3's own `unsendable` thread check,
  described in the design document. Convert Keleusma errors to Python exceptions that keep the source line and column.
  Never use `Vm::new_unchecked` to admit a module that verification rejects.
- **Python.** Typed throughout, checked by pyright in strict mode and ruff. The public
  interface lives in `python/keleusma/`.
- **Tests.** Each behaviour has a test, including the error paths. A test that derives both
  sides of a comparison is preferred to one that pins a constant.

## Writing Style

Prose is polite, professional and academic. No contractions. No em-dashes or en-dashes,
and no colons, semicolons or parentheticals in prose, with front matter, tables, code and
equations exempt. Spell out acronyms on first use. Distinguish facts, inferences and
hypotheses, and state uncertainty explicitly.
