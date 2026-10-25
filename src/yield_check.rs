//! A guard against yields whose value does not match the declared type.
//!
//! # Why the binding needs it
//!
//! Keleusma 0.2.2's compiler and verifier accept a `yield` whose expression has a
//! different type from the function's declared return type, which was observed on
//! 2026-10-25 and reported to the `v0.2.3` line. A `Text` field in a flat body is a
//! pointer and a length, and Keleusma's own decoder,
//! `<String as KeleusmaType>::from_flat_bytes_ctx`, passes those two words to an
//! `unsafe` constructor on the assumption that the type is right. A mistyped yield of
//! the same size as the declared type, for example `(3, "wake")` where `(Text, Word)`
//! is declared, therefore makes a safe read dereference the integer 3. The Python
//! process crashes with a segmentation fault. A mistyped yield of a different size is
//! caught by the length check in [`crate::convert`], but a same-size one cannot be
//! told apart from the bytes alone, and the VM exposes no address range for its
//! read-only data against which a pointer could be checked.
//!
//! KELEUSMA-0.2.2: Keleusma's PR 517 to the 0.2.3 branch makes its type checker
//! refuse a mistyped yield and checks a flat `Text` field's range before decoding.
//! Once a release carries it, this guard is redundant and may be removed.
//!
//! # What it checks
//!
//! The guard repeats the compiler's own sequence, a type check, monomorphisation and
//! the recording type check, which leaves Keleusma's inferred type for each
//! expression in `program.fn_expr_types`. For every `yield` in every function whose
//! declared return type the binding models, it compares the recorded type of the
//! yielded expression with the declared type. When a tuple or array literal has no
//! recorded type as a whole, its elements are compared one by one. A mismatch raises
//! `keleusma.CompileError` at the yield.
//!
//! # Values with no recorded type
//!
//! Keleusma records a type only where it is certain. Where it recorded none, and the
//! declared type holds `Text` inside a composite, the guard refuses the yield, because
//! a flat `Text` field is the pointer a mistyped value would make Keleusma's decoder
//! misread. One case is admitted, a name bound by `let name = yield ...` in `main`
//! when `main`'s resume type equals its declared yield type, because the binding
//! converted that resumed value by exactly that type. Where the declared type holds no
//! flat `Text`, a mistyped value is caught or misread without a pointer when it is
//! decoded, so it is let through, as is a recorded type the binding cannot model.
//!
//! # What it does not check
//!
//! The guard trusts the types Keleusma does record. It was run over the test suite,
//! the crate's example scripts and the book's programs without refusing any that
//! Keleusma runs correctly.

use std::collections::{BTreeMap, BTreeSet};

use keleusma::ast::{Expr, Pattern, Program, Stmt, TypeExpr};
use keleusma::monomorphize::monomorphize_with_provenance;
use keleusma::target::Target;
use keleusma::token::Span;
use keleusma::typecheck::{check_with_target, check_with_target_recording};
use keleusma::visitor::Visitor;

use crate::types::{EntrySignature, Resolver, Ty};

/// A yield whose value has a type other than the declared one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub message: String,
    pub span: Span,
}

/// Check every modelled yield in `program`. Returns the first mismatch found, in
/// source order of functions and then of yields, or an error message if the
/// re-check itself fails.
pub fn check(program: &Program, target: Target) -> Result<Option<Mismatch>, String> {
    let default_frac_bits = target.fixed_default_frac_bits();
    let mut owned = program.clone();
    check_with_target(&mut owned, target).map_err(|e| e.message)?;
    let (mut owned, _) = monomorphize_with_provenance(owned);
    check_with_target_recording(&mut owned, target).map_err(|e| e.message)?;
    let empty = BTreeMap::new();
    let resolver = Resolver::new(&owned, default_frac_bits);
    // The binding converts every resumed value by `main`'s declared resume type, so a
    // name bound to a yield's result in `main` holds a value of that type.
    let resume = EntrySignature::from_program(&owned, default_frac_bits)
        .and_then(|signature| signature.resume.ok());
    for function in &owned.functions {
        let Ok(declared) = resolver.resolve(&function.return_type) else {
            continue;
        };
        let resumes_as_declared =
            function.name == "main" && resume.as_ref().is_some_and(|r| *r == declared);
        let mut visitor = YieldVisitor {
            declared: &declared,
            recorded: owned.fn_expr_types.get(&function.name).unwrap_or(&empty),
            resolver: &resolver,
            function: &function.name,
            found: None,
            resumes_as_declared,
            resumed: BTreeSet::new(),
        };
        visitor.visit_block(&function.body);
        if visitor.found.is_some() {
            return Ok(visitor.found);
        }
    }
    Ok(None)
}

struct YieldVisitor<'a> {
    declared: &'a Ty,
    recorded: &'a BTreeMap<Span, TypeExpr>,
    resolver: &'a Resolver,
    function: &'a str,
    found: Option<Mismatch>,
    /// Whether this is `main` and its resume type equals its declared yield type.
    resumes_as_declared: bool,
    /// Names currently bound by `let name = yield ...`, which hold resumed values.
    resumed: BTreeSet<String>,
}

impl YieldVisitor<'_> {
    /// The recorded type of `expr`, if Keleusma recorded one the binding models.
    fn recorded_type(&self, expr: &Expr) -> Option<Ty> {
        let recorded = self.recorded.get(&expr.span())?;
        self.resolver.resolve(recorded).ok()
    }

    /// The first mismatch between `expr` and `expected`, descending into tuple and
    /// array literals whose type as a whole was not recorded.
    fn compare(&self, expr: &Expr, expected: &Ty) -> Option<Found> {
        if let Some(actual) = self.recorded_type(expr) {
            return (actual != *expected).then(|| Found::Mismatch(actual, expr.span()));
        }
        match (expr, expected) {
            (Expr::TupleLiteral { elements, .. }, Ty::Tuple(types))
                if elements.len() == types.len() =>
            {
                elements
                    .iter()
                    .zip(types)
                    .find_map(|(element, ty)| self.compare(element, ty))
            }
            (Expr::ArrayLiteral { elements, .. }, Ty::Array(element_ty, count))
                if elements.len() == *count =>
            {
                elements
                    .iter()
                    .find_map(|element| self.compare(element, element_ty))
            }
            // A resumed value, which the binding converted by the declared type.
            (Expr::Ident { name, .. }, _)
                if self.resumes_as_declared && self.resumed.contains(name) =>
            {
                None
            }
            // No recorded type and no literal to descend into. A flat `Text` field
            // is a raw pointer that a mistyped value would make Keleusma 0.2.2's
            // decoder dereference, so where `expected` holds one inside a composite
            // the value is refused rather than trusted. Elsewhere a wrong value is
            // caught safely when it is read, so it is let through.
            _ if holds_flat_text(expected, false) => Some(Found::Unknown(expr.span())),
            _ => None,
        }
    }
}

/// What the comparison found at a yield.
enum Found {
    /// A recorded type other than the declared one.
    Mismatch(Ty, Span),
    /// No recorded type where the declared type holds a flat `Text` field.
    Unknown(Span),
}

/// Whether a value of `ty` holds a `Text` field inside a composite, which Keleusma
/// stores flat as a pointer and a length. A `Text` on its own is a boxed value whose
/// type the reader checks, so it is safe. `nested` is whether `ty` is itself a field.
fn holds_flat_text(ty: &Ty, nested: bool) -> bool {
    match ty {
        Ty::Text => nested,
        Ty::Tuple(types) => types.iter().any(|t| holds_flat_text(t, true)),
        Ty::Array(element, _) => holds_flat_text(element, true),
        Ty::Struct { fields, .. } => fields.iter().any(|(_, t)| holds_flat_text(t, true)),
        Ty::Enum { variants, .. } => variants
            .iter()
            .flat_map(|v| v.fields.iter())
            .any(|t| holds_flat_text(t, true)),
        Ty::Option(inner) => holds_flat_text(inner, true),
        Ty::OutOnly { inner, .. } => holds_flat_text(inner, nested),
        Ty::Unit
        | Ty::Bool
        | Ty::Byte
        | Ty::Word
        | Ty::Fixed(_)
        | Ty::Float
        | Ty::Multiword { .. }
        | Ty::Opaque(_) => false,
    }
}

impl Visitor for YieldVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        self.walk_stmt(stmt);
        // Track the names that hold resumed values, in source order. Any other
        // binding of a name ends its tracking, so a shadowing `let` is not trusted.
        if let Stmt::Let(binding) = stmt
            && let Pattern::Variable(name, _) = &binding.pattern
        {
            if matches!(binding.value, Expr::Yield { .. }) {
                self.resumed.insert(name.clone());
            } else {
                self.resumed.remove(name);
            }
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if self.found.is_some() {
            return;
        }
        if let Expr::Yield { value, .. } = expr
            && let Some(found) = self.compare(value, self.declared)
        {
            self.found = Some(match found {
                Found::Mismatch(actual, span) => Mismatch {
                    message: format!(
                        "this yield in `{}` produces {} where the declared type is {}; \
                         Keleusma 0.2.2 accepts this, and reading it could crash, so the \
                         binding rejects it",
                        self.function,
                        actual.name(),
                        self.declared.name()
                    ),
                    span,
                },
                Found::Unknown(span) => Mismatch {
                    message: format!(
                        "Keleusma 0.2.2 records no type for this yielded value in `{}`, \
                         whose declared type {} holds text inside a composite, which a \
                         mistyped value would make Keleusma's decoder misread, so the \
                         binding rejects it; build the value as a literal, or bind it \
                         with `let` to a declared type first",
                        self.function,
                        self.declared.name()
                    ),
                    span,
                },
            });
            return;
        }
        self.walk_expr(expr);
    }
}

#[cfg(test)]
mod tests {
    use keleusma::lexer::tokenize;
    use keleusma::parser::parse;

    use super::*;

    fn check_source(source: &str) -> Option<Mismatch> {
        let program = parse(&tokenize(source).expect("lex")).expect("parse");
        check(&program, Target::host()).expect("re-check")
    }

    #[test]
    fn a_same_size_mistyped_yield_is_caught() {
        let source = "loop main(e: Word) -> (Text, Word) \
                      { if e > 0 { yield (\"a\", 1) } else { yield (3, \"wake\") } }";
        let mismatch = check_source(source).expect("mismatch");
        assert!(mismatch.message.contains("(Word, Text)"));
        assert!(mismatch.message.contains("(Text, Word)"));
        assert_eq!(mismatch.span.line, 1);
    }

    #[test]
    fn a_different_size_mistyped_yield_is_caught() {
        let source = "loop main(e: Word) -> (Text, Text) \
                      { if e > 0 { yield (\"send\", \"x\") } else { yield (\"wake\", 3) } }";
        assert!(check_source(source).is_some());
    }

    #[test]
    fn a_scalar_mistyped_yield_is_caught() {
        let source = "loop main(e: Word) -> Text { if e > 0 { yield \"a\" } else { yield 3 } }";
        assert!(check_source(source).is_some());
    }

    #[test]
    fn an_unrecorded_text_bearing_yield_is_refused() {
        // A resumed `Word` yielded where `(Text, Word)` is declared. Keleusma 0.2.2
        // records no type for `n` and accepts the program.
        let source = "loop main(e: Word) -> (Text, Word) \
                      { let n = yield (\"a\", e); let m = yield n; m }";
        assert!(check_source(source).is_some());
    }

    #[test]
    fn a_resumed_value_of_the_declared_type_may_be_yielded_again() {
        let source = "loop main(e: (Text, Word)) -> (Text, Word) \
                      { let n = yield e; let m = yield n; m }";
        assert!(check_source(source).is_none());
    }

    #[test]
    fn a_shadowed_resumed_name_is_not_trusted_as_resumed() {
        // `n` is rebound to a literal, whose recorded type is then what is compared.
        let ok = "loop main(e: (Text, Word)) -> (Text, Word) \
                  { let n = yield e; let n = (\"x\", 1); let m = yield n; m }";
        assert!(check_source(ok).is_none());
    }

    #[test]
    fn well_typed_yields_pass() {
        for source in [
            "loop main(e: (Text, Text)) -> (Text, Text) { let n = yield (\"send\", e.1); n }",
            "loop main(e: Word) -> Word { let a = yield e; let b = yield a + 1; b }",
            "loop main(e: Word) -> [(Text, Word); 2] { yield [(\"wake\", e), (\"end_turn\", 0)] }",
            "loop main(e: Word) -> (Text, Word) { let a = yield (\"x\", e); let b = yield (\"y\", a); b }",
            "fn f(x: Word) -> (Text, Word) { (\"t\", x) }\n\
             loop main(e: Word) -> (Text, Word) { let n = yield f(e); n }",
            "loop main(e: Fixed<8>) -> Fixed<8> { yield e + e }",
            "loop main(e: Word) -> Fixed { yield e as Fixed }",
            "loop main(e: Fixed) -> Fixed { yield e + e }",
            "loop main(e: Word) -> Fixed<8> { yield e as Fixed<8> }",
        ] {
            assert_eq!(check_source(source), None, "{source}");
        }
    }

    #[test]
    fn a_literal_element_is_compared_when_the_tuple_is_unrecorded() {
        // The second yield's tuple holds a value received from the first yield, so
        // Keleusma records no type for the tuple, and the literal element decides.
        let source = "loop main(e: Word) -> (Text, Word) \
                      { let a = yield (\"x\", e); let b = yield (a, \"y\"); b }";
        assert!(check_source(source).is_some());
    }
}
