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
//! # What it does not check
//!
//! Keleusma records a type only where it is certain, so an expression with no
//! recorded type, for example a tuple holding a value received from an earlier
//! `yield`, is not compared. That is a residual risk, stated in the design document,
//! that only a fix in Keleusma's type checker removes. The guard also treats a
//! recorded type it cannot model, such as a named type, as unknown, so a type alias
//! cannot cause a false rejection.

use std::collections::BTreeMap;

use keleusma::ast::{Expr, Program, TypeExpr};
use keleusma::monomorphize::monomorphize_with_provenance;
use keleusma::target::Target;
use keleusma::token::Span;
use keleusma::typecheck::{check_with_target, check_with_target_recording};
use keleusma::visitor::Visitor;

use crate::types::{Ty, resolve};

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
    for function in &owned.functions {
        let Ok(declared) = resolve(&function.return_type, default_frac_bits) else {
            continue;
        };
        let mut visitor = YieldVisitor {
            declared: &declared,
            recorded: owned.fn_expr_types.get(&function.name).unwrap_or(&empty),
            default_frac_bits,
            function: &function.name,
            found: None,
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
    default_frac_bits: u8,
    function: &'a str,
    found: Option<Mismatch>,
}

impl YieldVisitor<'_> {
    /// The recorded type of `expr`, if Keleusma recorded one the binding models.
    fn recorded_type(&self, expr: &Expr) -> Option<Ty> {
        let recorded = self.recorded.get(&expr.span())?;
        resolve(recorded, self.default_frac_bits).ok()
    }

    /// The first mismatch between `expr` and `expected`, descending into tuple and
    /// array literals whose type as a whole was not recorded.
    fn compare(&self, expr: &Expr, expected: &Ty) -> Option<(Ty, Span)> {
        if let Some(actual) = self.recorded_type(expr) {
            return (actual != *expected).then_some((actual, expr.span()));
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
            _ => None,
        }
    }
}

impl Visitor for YieldVisitor<'_> {
    fn visit_expr(&mut self, expr: &Expr) {
        if self.found.is_some() {
            return;
        }
        if let Expr::Yield { value, .. } = expr
            && let Some((actual, span)) = self.compare(value, self.declared)
        {
            let message = format!(
                "this yield in `{}` produces {} where the declared type is {}; \
                 Keleusma 0.2.2 accepts this, and reading it could crash, so the \
                 binding rejects it",
                self.function,
                actual.name(),
                self.declared.name()
            );
            self.found = Some(Mismatch { message, span });
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
