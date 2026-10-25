# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, roadmap approval and the sandbox settings

**Done.** The six approved roadmap changes for Dromon are in `docs/roadmap/V0_1_0.md`.
Tasks 3, 4, 5 and 8 grew and task 5b was added. The design document records the arena
sizing defect and the plan to decode composites by declared type. No code changed.

**Checked in the sandbox.**
- **Mailbox.** A write transaction on the interline store, rolled back, succeeded.
- **sccache.** `SCCACHE_SERVER_PORT` is 4227 and `SCCACHE_DIR` reaches the environment,
  but sccache reports the literal path `~/Library/Caches/Mozilla.sccache-agent`. It does
  not expand `~`, so the value needs an absolute path. The shared cache directory is
  still writable from the sandbox.
- **uv.** `python3` is now MacPorts Python 3.14, so pip installed uv 0.13.5 under
  `~/Library/Python/3.14/bin`, which is not on the `PATH`. The `PATH` still finds 0.9.5
  under `~/Library/Python/3.9/bin`, which belongs to `/usr/bin/python3`, Python 3.9.6.

**Writable paths.** The concrete changes are in the operator reply of this session and
summarised here. Inside the writable `~/.cargo`, deny writes to `bin`, `env`,
`config.toml`, `config` and `credentials.toml`, and deny reads of `credentials.toml`.
Remove the shared sccache cache from the writable list. Deny writes to `interline.py`
and `PROTOCOL.md` in the interline store. Optionally give uv a separate agent cache.

**Next.** Task 2, `keleusma.compile` with located exceptions.
