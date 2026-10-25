//! Every Keleusma call whose signature differs between 0.2.2 and the 0.2.3 branch.
//!
//! The binding pins `keleusma = "=0.2.2"`. A survey of the `origin/v0.2.3` branch on
//! 2026-10-25, recorded in `docs/architecture/MIGRATION_V0_2_3.md`, found that 0.2.3
//! adds an address width to the layout and flat-marshalling functions and new fields to
//! `RefContext`. Those calls live here, so a migration edits this file rather than every
//! conversion. Behavioural workarounds for 0.2.2 defects stay where they act and carry
//! the marker `KELEUSMA-0.2.2`, which the migration document lists.

use keleusma::Arena;
use keleusma::marshall::{KeleusmaType, RefContext};
use keleusma::value_layout::LayoutDescriptor;
use keleusma::vm::{NativeCtx, VmError};

/// The module's widths in bytes, which flat bodies are packed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Widths {
    pub word_bytes: usize,
    pub float_bytes: usize,
    /// The address width, which 0.2.3 uses to size an opaque field. Keleusma 0.2.2
    /// sizes an opaque field by the word width and takes no address width, so this is
    /// carried for the migration and does not yet affect any layout.
    pub addr_bytes: usize,
}

impl Widths {
    /// Widths from a module header, which stores each as the base-2 logarithm of a
    /// bit count.
    pub fn of(module: &keleusma::bytecode::Module) -> Self {
        let bytes = |log2: u8| (1usize << log2) / 8;
        Self {
            word_bytes: bytes(module.word_bits_log2),
            float_bytes: bytes(module.float_bits_log2),
            addr_bytes: bytes(module.addr_bits_log2),
        }
    }

    /// Widths from a native's context. Keleusma 0.2.2's `NativeCtx` has no address
    /// width, and 0.2.2 sizes opaques by the word width, so the word width stands in.
    /// KELEUSMA-0.2.2: on 0.2.3 this reads `ctx.addr_bytes`.
    pub fn of_native(ctx: &NativeCtx<'_>) -> Self {
        Self {
            word_bytes: ctx.word_bytes,
            float_bytes: ctx.float_bytes,
            addr_bytes: ctx.word_bytes,
        }
    }

    /// The size of a value with `layout` at these widths.
    /// KELEUSMA-0.2.2: on 0.2.3 `size_in_bytes` also takes `addr_bytes`.
    pub fn size_of(&self, layout: &LayoutDescriptor) -> usize {
        layout.size_in_bytes(self.word_bytes, self.float_bytes)
    }

    /// The offset of field `index` within a composite with `layout`.
    /// KELEUSMA-0.2.2: on 0.2.3 `field_offset` also takes `addr_bytes`.
    pub fn offset_of(&self, layout: &LayoutDescriptor, index: usize) -> Option<usize> {
        layout.field_offset(index, self.word_bytes, self.float_bytes)
    }
}

/// Read the text a flat `Text` field's two words name, under the epoch of the body
/// that holds it.
///
/// KELEUSMA-0.2.2: on 0.2.3 `RefContext` gains `addr_bytes`, and with PR 517 also
/// `rodata`, the VM's bytecode image, which Keleusma then checks the pointer against.
/// `from_flat_bytes_ctx` also gains `addr_bytes`. Keleusma exposes no accessor for the
/// bytecode image on either branch, which the migration document records as an open
/// question for Keleusma.
pub fn read_flat_text(
    bytes: &[u8],
    widths: Widths,
    arena: &Arena,
    epoch: u64,
) -> Result<String, VmError> {
    let refs = RefContext {
        arena,
        opaques: &[],
        word_bytes: widths.word_bytes,
        float_bytes: widths.float_bytes,
        ref_epoch: epoch,
    };
    <String as KeleusmaType<i64, f64>>::from_flat_bytes_ctx(
        bytes,
        widths.word_bytes,
        widths.float_bytes,
        &refs,
    )
}
