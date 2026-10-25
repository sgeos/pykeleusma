# Migration to Keleusma 0.2.3

> **Navigation**: [Architecture](./README.md) | [Documentation Root](../README.md)

The binding pins `keleusma = "=0.2.2"`. This document prepares the move to 0.2.3 for
when that release is published. It is based on a read-only survey made on 2026-10-25
of the `origin/v0.2.3` branch at `ec536b28`, and of `origin/fix/v02-flat-text-soundness`,
Keleusma's PR 517. The `v0.2.3` line reported in `v023-20261025-pr517-merged-py` that
PR 517 has since merged as `50b20ef6`, which this line has not verified, because it
does not fetch into the Keleusma repository. Items 3 and 11 assume it is in 0.2.3. Nothing here was built against those
branches, because the project builds only against published releases. Every item is
therefore a fact read from the branch source or an inference, as marked, and must be
confirmed against the published release.

## How the code is prepared

- **One file for changed signatures.** `src/compat.rs` holds every call whose
  signature differs on the branch, so items 2 and 3 edit that file only.
- **A marker at every workaround.** Each place that works around a 0.2.2 behaviour
  carries the comment `KELEUSMA-0.2.2`. `grep -rn KELEUSMA-0.2.2 src` lists them, and
  each names what to do on an upgrade.
- **Canaries.** `src/canaries.rs` pins each 0.2.2 behaviour the binding depends on,
  driven through Keleusma alone. After the version bump, a failing canary names its
  item below. Two more canaries guard the arena margin in `src/owned_vm.rs` and the
  yield guard in `src/yield_check.rs`.
- **A fuzz tool.** `tools/fuzz_load_module.py` exercises `load_module` and VM
  construction with mutated bytes in child processes and reports any crash.

## Procedure

1. Confirm that `keleusma` 0.2.3 and its new dependency `keleusma-wire` are published
   to crates.io. Inference. The binding cannot pin 0.2.3 until both are.
2. On a branch, change the pin in `Cargo.toml`, run `cargo update -p keleusma`, and
   rebuild with `uv sync`. Set the package version in `Cargo.toml` to 0.2.3 and the
   version in `pyproject.toml` to 0.2.3.0, which `tests/test_smoke.py` checks against
   the linked release.
3. Fix the compile errors, which the survey expects only in `src/compat.rs`, items 2
   and 3, and in the shared offset read, item 10.
4. Run `cargo test`. Work through each failing canary by its item below.
5. Run the full local gate from `CLAUDE.md`, then `tools/fuzz_load_module.py
   --batches 300`.
6. Remove each `KELEUSMA-0.2.2` marker whose item is resolved, and update the
   README, the design and the CHANGELOG.

## Items

1. **Bytecode version 2.** Fact. `BYTECODE_VERSION` becomes 2, and version 1 bytes
   fail to load with `LoadError::UnsupportedVersion`. Every `.kel.bin` must be
   recompiled. Update `tests/test_smoke.py`, which pins version 1, and the canary
   `bytecode_version_is_1`.
2. **Address width.** Fact. `LayoutDescriptor::size_in_bytes` and `field_offset`, and
   `KeleusmaType::from_flat_bytes_ctx`, gain an `addr_bytes` parameter, and
   `RefContext` and `NativeCtx` gain an `addr_bytes` field. In `src/compat.rs`, pass
   `self.addr_bytes` in `Widths::size_of` and `Widths::offset_of`, read
   `ctx.addr_bytes` in `Widths::of_native`, and add the field to the `RefContext`
   literal in `read_flat_text`. `Widths` already carries `addr_bytes`.
3. **Read-only data, PR 517.** Fact. With PR 517, `RefContext` and `NativeCtx` gain
   `rodata`, the VM's bytecode image, and a flat `Text` field must lie inside the
   arena or that image. No public accessor for the image exists on either branch.
   Inference. With `rodata: &[]`, the binding would refuse a returned composite whose
   text is a string literal. This needs an accessor from Keleusma, which was asked for.
4. **Signed hot swap.** Fact. Fixed on the branch. The canary
   `signed_hot_swap_fails_on_every_signed_module` will fail. `replace_module_signed` in
   `src/owned_vm.rs` may then delegate to `Vm::replace_module_from_bytes`, keeping the
   binding's trust matrix and arena check.
5. **Trust matrix on a plain swap.** Fact. Unchanged on the branch. Keep the check in
   `OwnedVm::replace_module`.
6. **Arena check on a swap.** Fact. Unchanged on the branch. Keep `check_fits`.
7. **Text concatenation.** Fact. The branch refuses `Text + Text` at compile time,
   where 0.2.2 compiles it and fails at run time. Update the design's known issues.
8. **Private data initialisation.** Fact. A fresh VM starts scalar private slots at
   zero or their initialiser, from `DataLayout::private_init`. Make
   `OwnedVm::fresh_private_data` build those values with
   `GenericValue::scalar_from_const`, with `()` for composites, so a hot swap matches
   a fresh VM. Update the tests that expect `()`.
9. **Const-generic enum layout.** Not surveyed. Re-check the canary
   `a_const_generic_enum_records_only_the_generic_layout`. If Keleusma records the
   instance layout, remove `OutOnly::GenericEnum` in `src/types.rs`. Re-check
   type-generic instances, which `Resolver::named` refuses, the same way.
10. **Shared slot offsets.** Fact. `SharedSlotLayout::offset` widens from `u16` to
    `u32`. Change `compat::shared_slot_offset` to `usize::try_from(slot.offset).ok()`.
11. **Yield guard.** Fact. PR 517 makes the type checker refuse a mistyped yield, and
    it is reported merged. Once the release that includes it is pinned,
    `src/yield_check.rs` is redundant. Check that `keleusma.compile` refuses the
    mistyped yields in that file's tests through Keleusma alone, then remove the guard.
12. **Arena margin.** Fact. The branch corrects three understatements in the
    worst-case memory model. Inference, untested. The shortfall after the first
    `Reset` may close. The canary `the_verifier_footprint_alone_fails_after_the_first_reset`
    will say. Re-measure the margin before removing it.
13. **Bounds rise.** Fact. Worst-case figures rise for affected chunks. Tests compare
    figures rather than pinning them, so they should hold, but arena sizes grow.
14. **Opcodes.** Fact. The data operands widen. Inference. No variant is renamed.
    Compare `compat::OP_NAMES` with the `Op` enum, which the test
    `every_op_in_the_crate_examples_is_named` checks only for ops the examples use.
15. **New error variant.** Fact. `VmError::LoopLimitExceeded` is added. It reaches
    Python through `vm_error`'s general arm as `VMError`. Consider a clearer message.
16. **Stricter compilation.** Fact. The category-call discipline may reject programs
    that compiled before. Run the test suite and the book survey again.
17. **External natives.** Fact. `register_external_native` keeps its function-pointer
    signature on the branch. `compat::native_ctx` builds the context the trampolines
    pass, and gains `addr_bytes`, and `rodata` with PR 517.
18. **Type-generic structs.** Re-check whether a host-built instance is read as its
    fields, and if so remove `OutOnly::TypeGeneric`.
19. **Private array stores.** Not surveyed. The canary
    `a_whole_array_store_to_private_data_fills_only_the_first_slot` pins the 0.2.2
    defect, and `VM.private_data` reads the misplaced array. If the canary fails, remove
    that special case. The private slot layout `OwnedVm::private_slots` reads is pinned
    by `private_slots_read_back_what_the_script_wrote`.
20. **Unchanged paths the binding relies on.** Fact. `Vm::decode` and
    `Value::OpaqueRef`, which `compat::resolve_opaque` uses, the cost model API, the
    `wire_format` functions and the Cargo features are unchanged.

## Requests to Keleusma that would remove workarounds

- An accessor for the bytecode image, for item 3.
- An accessor for the opaque registry, replacing `compat::resolve_opaque`.
- An accessor for private slot values, replacing `OwnedVm::private_slots`.
- A closure-based `register_external_native` with a native context, which would
  retire the trampoline pool in `src/external.rs`.
- A host-side halting `VmError` variant, replacing the `InvalidBytecode` carrier in
  `src/natives.rs`.
- Fixes for items 5, 6 and 9, and for the footprint analysis's unbounded recursion on
  an unverified module.
