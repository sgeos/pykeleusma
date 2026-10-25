# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, the sync with the project template

**Asked.** Sync the repository with `sgeos/project-template`, with the code of conduct
as the high priority, make sure the GitHub repository is in order, audit the template,
and draft a message for the administrative line if the audit warrants one.

**Synced, against the template at `227f261`.**
- `CODE_OF_CONDUCT.md` unchanged from the template.
- `CONTRIBUTING.md`, `CITATION.cff` and `llms.txt`, adapted to the binding, with the
  guarantee boundary and the line between this repository and Keleusma.
- `.github/CODEOWNERS`, `dependabot.yml` for the pinned actions only, `release.yml`,
  and `rulesets/main.json`, which records the live ruleset. Cargo and uv stay out of
  Dependabot because their pins are deliberate, as the file says.
- `scripts/check.sh` with the template's hygiene section unchanged, followed by the
  project's checks, and `--hygiene-only` to stop before them. Also the pre-push hook,
  `scripts/worktree.sh`, `.gitattributes`, the decision template, and the template's
  `.editorconfig` and `.gitignore` entries.
- CI gains a repository job running the hygiene checks, actionlint and zizmor. Every
  job has a timeout and only the permissions it needs. The concurrency group no longer
  lets a second merge to `main` cancel the first merge's runs, which it could before.
  Two `ls | wc -l` counts became array counts, because actionlint without the local
  ignore flag reported them. zizmor's one finding, an expression inside a run step, now
  goes through an environment variable.

**GitHub settings changed.** Squash and rebase merging are disabled, so only merge
commits remain, matching the process. The labels `breaking` and `ignore-for-release`
were created for `release.yml`. Everything else already matched the template's step 9.

**Kept different from the template, deliberately.** The reduced pull request set, the
absent `Co-Authored-By` trailer, and `.claude/` ignored whole.

**Audit.** The findings went to the operator as a draft for `Op55-admin-polysabite`,
not into the repository.

**Verified.** The local gate passes, with 411 Python and 49 Rust tests. actionlint and
zizmor 1.31.1 report nothing on either workflow. The pull request results are recorded
in the commit history, not here.
