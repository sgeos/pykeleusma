# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, coverage of the Keleusma book, task 12

**Goal.** Make `pykeleusma` act as a bridge for all functionality the Keleusma book
describes, and report anything broken. The operator also corrected the units of
`wcet_per_iteration` to nominal cycles.

**Done.** Structs, enums, options and newtypes convert at the boundary. Natives report
recoverable errors through `keleusma.NativeError`, and `vm.reset()` recovers a failed
VM. `libraries=` offers the bundled `math`, `audio` and `shell` natives. Modules
serialise, sign, encrypt and load, and `vm.replace_module` swaps them with a trust
matrix. The crate's ten example scripts give their stated results. The units now read
nominal cycles everywhere, and `pydromon` was told.

**Defects found in the binding and fixed.** A script's native-error handler caught every
Python exception, with the code -1, which would have swallowed a `KeyboardInterrupt`.
A native error the script handled could mask a later runtime error in the same call.
Both have tests that fail without the fix.

**Defects found in Keleusma 0.2.2, reported to `v0.2.3` and worked around.** Signed hot
swap fails on every signed module. The plain swap ignores the trust matrix. The swap's
resource check admits a module that construction would refuse. The FAQ says text
doubling with `s + s` runs, and in 0.2.2 it fails with a type error. An earlier version
of the report said the swap made no resource check at all, which was wrong and was
corrected in a second message.

**Verification.** 325 Python tests and 29 Rust tests pass locally, with pyright, ruff,
rustfmt and clippy clean. Seven mutations of the new safeguards were each caught. Of
the 73 programs in the book, 41 were run, and the other 32 were not checked further.

**Not supported.** Listed in `HANDOFF.md` under Open and in the design.
