//! Every Keleusma call whose signature differs between 0.2.2 and the 0.2.3 branch.
//!
//! The binding pins `keleusma = "=0.2.2"`. A survey of the `origin/v0.2.3` branch on
//! 2026-10-25, recorded in `docs/architecture/MIGRATION_V0_2_3.md`, found that 0.2.3
//! adds an address width to the layout and flat-marshalling functions and new fields to
//! `RefContext`. Those calls live here, so a migration edits this file rather than every
//! conversion. Behavioural workarounds for 0.2.2 defects stay where they act and carry
//! the marker `KELEUSMA-0.2.2`, which the migration document lists.

use std::cell::RefCell;
use std::collections::BTreeMap;

use keleusma::Arena;
use keleusma::bytecode::{CostModel, NOMINAL_COST_MODEL, Op};
use keleusma::marshall::{KeleusmaType, RefContext};
use keleusma::opaque::HostOpaque;
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

/// Resolve an opaque registry index through `decode`, Keleusma's only public path to
/// the registry.
///
/// KELEUSMA-0.2.2: the index is wrapped in `Value::OpaqueRef`, a variant Keleusma
/// marks as hidden internal form, because no accessor exists. The 0.2.3 branch keeps
/// both the variant and `decode` unchanged. A Keleusma accessor would replace this.
pub fn resolve_opaque(
    index: usize,
    decode: impl FnOnce(&keleusma::bytecode::Value) -> Option<std::sync::Arc<dyn HostOpaque>>,
) -> Option<std::sync::Arc<dyn HostOpaque>> {
    let index = u32::try_from(index).ok()?;
    decode(&keleusma::bytecode::Value::OpaqueRef(index))
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

/// The name of every `Op` variant in Keleusma 0.2.2, the names a cost table keys.
/// KELEUSMA-0.2.2: 0.2.3 may add variants, and a test fails if a compiled module
/// holds an op whose name is missing here.
pub const OP_NAMES: [&str; 66] = [
    "Const",
    "GetLocal",
    "SetLocal",
    "GetData",
    "SetData",
    "GetDataIndexed",
    "SetDataIndexed",
    "BoundsCheck",
    "Add",
    "Sub",
    "Mul",
    "Div",
    "Mod",
    "Neg",
    "CmpEq",
    "CmpNe",
    "CmpLt",
    "CmpGt",
    "CmpLe",
    "CmpGe",
    "Not",
    "If",
    "Else",
    "EndIf",
    "Loop",
    "EndLoop",
    "Break",
    "BreakIf",
    "Stream",
    "Reset",
    "Call",
    "Return",
    "Yield",
    "Dup",
    "NewComposite",
    "GetField",
    "GetIndex",
    "GetTupleField",
    "GetEnumField",
    "Len",
    "IsEnum",
    "IsStruct",
    "IntToFloat",
    "FloatToInt",
    "WordToByte",
    "ByteToWord",
    "WordToFixed",
    "FixedToWord",
    "FixedMul",
    "FixedDiv",
    "Trap",
    "CheckedAdd",
    "CheckedSub",
    "CheckedMul",
    "CheckedNeg",
    "CheckedDiv",
    "CheckedMod",
    "PushImmediate",
    "PopN",
    "BitAnd",
    "BitOr",
    "BitXor",
    "Shl",
    "Shr",
    "CallVerifiedNative",
    "CallExternalNative",
];

/// The variant name of `op`, the part of its debug form before any operand.
pub fn op_name(op: &Op) -> String {
    let debug = format!("{op:?}");
    let end = debug.find(['(', ' ', '{']).unwrap_or(debug.len());
    debug[..end].to_string()
}

thread_local! {
    /// The cost table in force for the analysis running on this thread. Keleusma's
    /// `CostModel` takes a plain function pointer, which cannot capture a table, so
    /// [`with_cost_table`] installs one here for exactly the duration of a call.
    static COST_TABLE: RefCell<Option<BTreeMap<String, u32>>> = const { RefCell::new(None) };
}

/// The cost of `op` in the installed table, or its nominal cost if the table does
/// not name it or none is installed.
fn table_op_cycles(op: &Op) -> u32 {
    let named = COST_TABLE.with(|table| {
        table
            .borrow()
            .as_ref()
            .and_then(|t| t.get(&op_name(op)).copied())
    });
    named.unwrap_or_else(|| (NOMINAL_COST_MODEL.op_cycles)(op))
}

/// Run `analysis` with a cost model whose per-op costs come from `table`, falling
/// back to the nominal model, and whose per-text-byte cost is `text_byte_cycles`.
///
/// The previous table is restored afterwards, even if `analysis` panics, so a nested
/// analysis on the same thread cannot leave a stale table behind.
pub fn with_cost_table<R>(
    table: BTreeMap<String, u32>,
    text_byte_cycles: Option<u32>,
    analysis: impl FnOnce(&CostModel) -> R,
) -> R {
    struct Restore(Option<BTreeMap<String, u32>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            COST_TABLE.with(|t| *t.borrow_mut() = previous);
        }
    }
    let previous = COST_TABLE.with(|t| t.borrow_mut().replace(table));
    let _restore = Restore(previous);
    let model = CostModel {
        value_slot_bytes: NOMINAL_COST_MODEL.value_slot_bytes,
        op_cycles: table_op_cycles,
        text_byte_cycles: text_byte_cycles.unwrap_or(NOMINAL_COST_MODEL.text_byte_cycles),
    };
    analysis(&model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_op_in_the_crate_examples_is_named() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/keleusma_scripts");
        let mut seen = 0usize;
        for entry in std::fs::read_dir(dir).expect("scripts") {
            let path = entry.expect("entry").path();
            if path.extension().is_none_or(|e| e != "kel") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("read");
            let tokens = keleusma::lexer::tokenize(&source).expect("lex");
            let program = keleusma::parser::parse(&tokens).expect("parse");
            let module = keleusma::compiler::compile(&program).expect("compile");
            for op in module.chunks.iter().flat_map(|c| c.ops.iter()) {
                let name = op_name(op);
                assert!(OP_NAMES.contains(&name.as_str()), "{name} is not named");
                seen += 1;
            }
        }
        assert!(seen > 100, "only {seen} ops were checked");
    }

    #[test]
    fn a_table_overrides_named_ops_and_is_removed_afterwards() {
        let add = Op::Add;
        let nominal = (NOMINAL_COST_MODEL.op_cycles)(&add);
        let table = BTreeMap::from([("Add".to_string(), nominal + 100)]);
        let inside = with_cost_table(table, None, |model| (model.op_cycles)(&add));
        assert_eq!(inside, nominal + 100);
        assert_eq!(table_op_cycles(&add), nominal);
    }
}
