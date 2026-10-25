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

**Status.** Cut on 2026-10-24. The extension builds against the published Keleusma 0.2.2.
Task 1 of the first milestone, owning the arena and the VM together, is done, and the
public package still exposes version information only. The first milestone is
[`docs/roadmap/V0_1_0.md`](docs/roadmap/V0_1_0.md) and the design is
[`docs/architecture/BINDING_DESIGN.md`](docs/architecture/BINDING_DESIGN.md).

**Relationship to Keleusma.** This repository depends only on the crates.io release, pinned
exactly as `keleusma = "=0.2.2"`. Never build against a Keleusma checkout and never edit
the Keleusma repository. Moving to a later release is a deliberate change, because
Keleusma 0.2.3 changes `BYTECODE_VERSION` from 1 to 2. Questions about Keleusma go to the
Keleusma lines through the mailbox described in `docs/process/HANDOFF.md`.

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

`main` is always green. Work happens on short-lived feature branches merged into `main` by
a no-fast-forward merge once checks pass, through a pull request once a remote exists. Use
scoped conventional commits such as `feat:`, `fix:`, `docs:`, `test:` and `chore:`.
Attribute assisted commits to the assistant that actually contributed, with
`Co-Authored-By: Claude <noreply@anthropic.com>` for Claude and
`Co-Authored-By: Codex <noreply@openai.com>` for Codex.

**Irreversible or outward-facing actions need the operator's confirmation in session.**
That includes creating or pushing to a remote, publishing to PyPI or crates.io, and
force-pushing.

## Common Commands

```sh
uv sync                                       # build the extension and install dev tools
uv run pytest                                 # tests
uv run ruff check . && uv run ruff format --check .
uv run pyright                                # strict type checks
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                                    # Rust unit tests
MIRIFLAGS=-Zmiri-tree-borrows CARGO_TARGET_DIR=target/miri cargo +nightly miri test --lib
```

`uv sync` rebuilds the extension when Rust sources change, through the `cache-keys` in
`pyproject.toml`. Miri runs under Tree Borrows because of the dependency issue recorded
in the design document. After editing `src/lib.rs`, keep
`python/keleusma/_keleusma.pyi` in step, because pyright reads the stub, not the binary.

## Before Trusting a Green Run

- **Know what the command covered.** A test run that imported a stale build of the
  extension tested the old code. Rebuild, then test.
- **Count what was reached.** A suite that collected zero tests, or skipped them all, passes.
  Check the count in the summary line.

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
