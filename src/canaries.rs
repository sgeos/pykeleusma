//! Canaries for Keleusma 0.2.2 behaviours the binding works around.
//!
//! Each test pins one behaviour of the pinned crate, driven through Keleusma alone,
//! with no binding code. A test that fails after an upgrade means the behaviour
//! changed, and the item it names in `docs/architecture/MIGRATION_V0_2_3.md` says what
//! the binding should then change. A canary is not a statement that the behaviour is
//! right. Several pin defects reported to the `v0.2.3` line.
//!
//! Two further canaries live beside the code they guard, the arena shortfall after the
//! first `Reset` in `owned_vm.rs` and the mistyped-yield tests in `yield_check.rs`.

#![cfg(test)]

use ed25519_dalek::SigningKey;
use keleusma::Arena;
use keleusma::bytecode::{BYTECODE_VERSION, Module, Value};
use keleusma::vm::{Vm, VmError};
use keleusma::wire_format;

fn module(source: &str) -> Module {
    let tokens = keleusma::lexer::tokenize(source).expect("lex");
    let program = keleusma::parser::parse(&tokens).expect("parse");
    keleusma::compiler::compile(&program).expect("compile")
}

const SEE: &str = "see docs/architecture/MIGRATION_V0_2_3.md";

#[test]
fn bytecode_version_is_1() {
    // Migration item 1. 0.2.3 writes version 2 and refuses version 1.
    assert_eq!(BYTECODE_VERSION, 1, "{SEE}, item 1");
}

#[test]
#[cfg_attr(miri, ignore = "Ed25519 signing is too slow under Miri")]
fn signed_hot_swap_fails_on_every_signed_module() {
    // Migration item 4. The binding verifies signed swaps itself. The survey found
    // this fixed on the 0.2.3 branch, after which the binding may delegate.
    let key = SigningKey::from_bytes(&[7; 32]);
    let update = module("signed loop main(t: Word) -> Word { let n = yield t + 1; n }");
    let bytes = wire_format::module_to_signed_wire_bytes(&update, &key).expect("sign");
    let arena = Arena::with_capacity(1 << 20);
    let mut vm = Vm::new(
        module("loop main(t: Word) -> Word { let n = yield t; n }"),
        &arena,
    )
    .expect("vm");
    vm.register_verifying_key(key.verifying_key());
    let result = vm.replace_module_from_bytes(&bytes, vec![]);
    assert!(
        matches!(&result, Err(VmError::LoadError(m)) if m.contains("FLAG_REQUIRES_SIGNATURE")),
        "{SEE}, item 4: {result:?}"
    );
}

#[test]
#[cfg_attr(miri, ignore = "Ed25519 key derivation is too slow under Miri")]
fn plain_hot_swap_ignores_the_trust_matrix() {
    // Migration item 5. The binding refuses an unsigned swap once a key is trusted.
    // The survey found this unchanged on the 0.2.3 branch.
    let key = SigningKey::from_bytes(&[7; 32]);
    let arena = Arena::with_capacity(1 << 20);
    let mut vm = Vm::new(
        module("loop main(t: Word) -> Word { let n = yield t; n }"),
        &arena,
    )
    .expect("vm");
    vm.register_verifying_key(key.verifying_key());
    let plain = module("loop main(t: Word) -> Word { let n = yield t + 2; n }");
    assert!(vm.replace_module(plain, vec![]).is_ok(), "{SEE}, item 5");
}

#[test]
fn hot_swap_admits_a_module_construction_refuses() {
    // Migration item 6. The binding checks a swapped module by its construction rule.
    // The survey found this unchanged on the 0.2.3 branch.
    let big = || {
        module(
            "fn main(t: Word) -> Word \
             { let a = [t, t, t, t, t, t, t, t, t, t, t, t, t, t, t, t]; a[0] }",
        )
    };
    let arena = Arena::with_capacity(256);
    assert!(Vm::new(big(), &arena).is_err(), "{SEE}, item 6");
    let mut vm = Vm::new(module("fn main(t: Word) -> Word { t }"), &arena).expect("vm");
    assert!(vm.replace_module(big(), vec![]).is_ok(), "{SEE}, item 6");
    assert!(
        matches!(vm.call(&[Value::Int(1)]), Err(VmError::OutOfArena(_))),
        "{SEE}, item 6"
    );
}

#[test]
fn text_concatenation_compiles_and_fails_at_run_time() {
    // Migration item 7. 0.2.3 refuses `Text + Text` at compile time instead.
    let arena = Arena::with_capacity(1 << 20);
    let mut vm = Vm::new(
        module("fn main() -> Text { let s = \"a\"; let s = s + s; s }"),
        &arena,
    )
    .expect("vm");
    let result = vm.call(&[]);
    assert!(
        matches!(&result, Err(VmError::TypeError(m)) if m.contains("KStr")),
        "{SEE}, item 7"
    );
}

#[test]
fn private_data_starts_as_unit() {
    // Migration item 8. 0.2.3 starts scalar private slots at zero or their
    // initialiser, and the binding's hot swap should then seed the same values.
    let source = "private data st { n: Word }\nfn main() -> Word { st.n = st.n + 1; st.n }";
    let private = module(source);
    let mut arena = Arena::with_capacity(1 << 20);
    arena
        .resize_persistent(keleusma::vm::required_persistent_capacity_for(&private))
        .expect("persistent");
    let mut vm = Vm::new(private, &arena).expect("vm");
    assert!(
        matches!(vm.call(&[]), Err(VmError::TypeError(_))),
        "{SEE}, item 8"
    );
}

#[test]
fn a_whole_array_store_to_private_data_fills_only_the_first_slot() {
    // Migration item 19. Element-wise stores work, and a later read of the second
    // element after a whole-array store gives `()`.
    let source = "private data pv { a: [Word; 2] }\n\
                  fn main(x: Word) -> Word { pv.a = [x, x * 2]; pv.a[1] }";
    let private = module(source);
    let mut arena = Arena::with_capacity(1 << 20);
    arena
        .resize_persistent(keleusma::vm::required_persistent_capacity_for(&private))
        .expect("persistent");
    let mut vm = Vm::new(private, &arena).expect("vm");
    let result = vm.call(&[Value::Int(3)]).map(|s| format!("{s:?}"));
    assert!(
        matches!(&result, Ok(s) if s.contains("Unit")),
        "{SEE}, item 19: {result:?}"
    );
}

#[test]
fn a_const_generic_enum_records_only_the_generic_layout() {
    // Migration item 9. The binding refuses host-built const-generic enum values,
    // because their padding is not recorded.
    let generic =
        module("enum E<const n: Word> { A([Word; n]), B }\nfn main(e: E<2>) -> Word { 0 }");
    let layouts: Vec<_> = generic
        .enum_layouts
        .iter()
        .map(|l| (l.type_name.as_str(), l.min_payload))
        .collect();
    assert_eq!(layouts, [("E", 0)], "{SEE}, item 9");
}
