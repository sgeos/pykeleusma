//! The declared types of a script's entry point, read from the parsed program.
//!
//! Roadmap task 4 converts values by type. A flat Keleusma composite carries no
//! per-field types, so the binding reads them from the entry point's declaration,
//! which `keleusma.compile` has in hand as the parsed program.
//!
//! Only the types 0.1.0 converts are modelled, which are the primitives, unit,
//! tuples and arrays. A named type, an option, an untyped parameter or any other
//! form is recorded as [`Unsupported`] with a description, so a module that uses one
//! still compiles and builds a VM, and the error appears only when a value of that
//! type is converted.
//!
//! Layouts come from Keleusma's own [`LayoutDescriptor`], which is also what the
//! compiler uses through its type checker, so field offsets match the compiled code
//! by construction rather than by a copy of its rules.

use keleusma::ast::{ConstExpr, FunctionCategory, PrimType, Program, TypeExpr};
use keleusma::value_layout::{LayoutDescriptor, ScalarKind};

/// A type the binding converts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Unit,
    Bool,
    Byte,
    Word,
    /// `Fixed<N>`, with `N` resolved, including the target default for bare `Fixed`.
    Fixed(u8),
    Float,
    Text,
    Tuple(Vec<Ty>),
    Array(Box<Ty>, usize),
}

/// Why a declared type cannot be converted, for the error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported(pub String);

/// A declared type, converted or not.
pub type Declared = Result<Ty, Unsupported>;

impl Ty {
    /// The layout Keleusma uses for this type.
    pub fn layout(&self) -> LayoutDescriptor {
        match self {
            Ty::Unit => LayoutDescriptor::Scalar(ScalarKind::Unit),
            Ty::Bool => LayoutDescriptor::Scalar(ScalarKind::Bool),
            Ty::Byte => LayoutDescriptor::Scalar(ScalarKind::Byte),
            Ty::Word => LayoutDescriptor::Scalar(ScalarKind::Int),
            Ty::Fixed(_) => LayoutDescriptor::Scalar(ScalarKind::Fixed),
            Ty::Float => LayoutDescriptor::Scalar(ScalarKind::Float),
            Ty::Text => LayoutDescriptor::Scalar(ScalarKind::Text),
            Ty::Tuple(elements) => {
                LayoutDescriptor::Tuple(elements.iter().map(Ty::layout).collect())
            }
            Ty::Array(element, count) => LayoutDescriptor::Array {
                element: Box::new(element.layout()),
                count: *count,
            },
        }
    }

    /// A short name for error messages, in Keleusma's surface syntax.
    pub fn name(&self) -> String {
        match self {
            Ty::Unit => "()".to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::Byte => "Byte".to_string(),
            Ty::Word => "Word".to_string(),
            Ty::Fixed(n) => format!("Fixed<{n}>"),
            Ty::Float => "Float".to_string(),
            Ty::Text => "Text".to_string(),
            Ty::Tuple(elements) => {
                let names: Vec<String> = elements.iter().map(Ty::name).collect();
                format!("({})", names.join(", "))
            }
            Ty::Array(element, count) => format!("[{}; {count}]", element.name()),
        }
    }
}

/// Resolve a declared type expression.
///
/// `default_frac_bits` is the target's default for a bare `Fixed`, which the type
/// checker substitutes the same way.
pub fn resolve(expr: &TypeExpr, default_frac_bits: u8) -> Declared {
    match expr {
        TypeExpr::Unit(_) => Ok(Ty::Unit),
        TypeExpr::Prim(prim, _) => Ok(match prim {
            PrimType::Bool => Ty::Bool,
            PrimType::Byte => Ty::Byte,
            PrimType::Word => Ty::Word,
            PrimType::Fixed(bits) => Ty::Fixed(bits.unwrap_or(default_frac_bits)),
            PrimType::Float => Ty::Float,
            PrimType::Text => Ty::Text,
        }),
        TypeExpr::Tuple(elements, _) => elements
            .iter()
            .map(|e| resolve(e, default_frac_bits))
            .collect::<Result<Vec<_>, _>>()
            .map(Ty::Tuple),
        TypeExpr::Array(element, length, _) => {
            let element = resolve(element, default_frac_bits)?;
            match length {
                ConstExpr::Lit(n, _) => usize::try_from(*n)
                    .map(|count| Ty::Array(Box::new(element), count))
                    .map_err(|_| Unsupported(format!("an array length of {n}"))),
                _ => Err(Unsupported(
                    "an array whose length is not a literal".to_string(),
                )),
            }
        }
        // A label narrows a type without changing its representation.
        TypeExpr::Labelled(inner, _, _) => resolve(inner, default_frac_bits),
        TypeExpr::Named(name, _, _, _) => Err(Unsupported(format!(
            "the named type `{name}`, since structs and enums arrive after 0.1.0"
        ))),
        TypeExpr::Option(_, _) => Err(Unsupported(
            "an option type, which arrives after 0.1.0".to_string(),
        )),
        TypeExpr::Multiword(_, _, _) => Err(Unsupported("a multiword integer".to_string())),
        #[allow(unreachable_patterns)]
        _ => Err(Unsupported("this type form".to_string())),
    }
}

/// The declared types at the entry point's boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySignature {
    /// Whether `main` is a plain function, a `yield` coroutine or a `loop` stream.
    pub category: Category,
    /// Each parameter's type, in order.
    pub params: Vec<Declared>,
    /// The return type, which is also the type of every yielded value.
    pub ret: Declared,
}

/// The kind of entry point, mirroring Keleusma's `FunctionCategory`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Fn,
    Yield,
    Loop,
}

impl EntrySignature {
    /// Read the signature of `main` from `program`, or `None` if it has no `main`.
    ///
    /// A function may have several heads, selected by pattern. All heads must declare
    /// the same types, or the boundary has no single type and is recorded as
    /// unsupported.
    pub fn from_program(program: &Program, default_frac_bits: u8) -> Option<Self> {
        let mut heads = program.functions.iter().filter(|f| f.name == "main");
        let first = heads.next()?;
        let signature_of = |f: &keleusma::ast::FunctionDef| EntrySignature {
            category: match f.category {
                FunctionCategory::Fn => Category::Fn,
                FunctionCategory::Yield => Category::Yield,
                FunctionCategory::Loop => Category::Loop,
            },
            params: f
                .params
                .iter()
                .map(|p| match &p.type_expr {
                    Some(expr) => resolve(expr, default_frac_bits),
                    None => Err(Unsupported(
                        "a parameter without a declared type".to_string(),
                    )),
                })
                .collect(),
            ret: resolve(&f.return_type, default_frac_bits),
        };
        let signature = signature_of(first);
        if heads.any(|other| signature_of(other) != signature) {
            let mixed = || {
                Err(Unsupported(
                    "heads of `main` with different types".to_string(),
                ))
            };
            return Some(EntrySignature {
                category: signature.category,
                params: signature.params.iter().map(|_| mixed()).collect(),
                ret: mixed(),
            });
        }
        Some(signature)
    }
}

#[cfg(test)]
mod tests {
    use keleusma::lexer::tokenize;
    use keleusma::parser::parse;

    use super::*;

    const DEFAULT: u8 = 32;

    fn signature(source: &str) -> EntrySignature {
        let program = parse(&tokenize(source).expect("lex")).expect("parse");
        EntrySignature::from_program(&program, DEFAULT).expect("main")
    }

    #[test]
    fn primitives_and_composites_resolve() {
        let s = signature(
            "loop main(e: (Text, Word, [Fixed<8>; 2], Byte, bool, Float, Fixed)) -> () { yield () }",
        );
        assert_eq!(s.category, Category::Loop);
        assert_eq!(
            s.params,
            vec![Ok(Ty::Tuple(vec![
                Ty::Text,
                Ty::Word,
                Ty::Array(Box::new(Ty::Fixed(8)), 2),
                Ty::Byte,
                Ty::Bool,
                Ty::Float,
                Ty::Fixed(DEFAULT),
            ]))]
        );
        assert_eq!(s.ret, Ok(Ty::Unit));
    }

    #[test]
    fn a_named_type_is_unsupported_but_does_not_fail_the_signature() {
        let s = signature("struct P { x: Word }\nfn main(p: P, n: Word) -> Word { n }");
        assert!(matches!(&s.params[0], Err(Unsupported(m)) if m.contains("`P`")));
        assert_eq!(s.params[1], Ok(Ty::Word));
        assert_eq!(s.ret, Ok(Ty::Word));
    }

    #[test]
    fn a_program_without_main_has_no_signature() {
        let program = parse(&tokenize("fn other() -> Word { 1 }").expect("lex")).expect("parse");
        assert_eq!(EntrySignature::from_program(&program, DEFAULT), None);
    }

    #[test]
    fn layouts_match_keleusma_sizes() {
        // Text is two words, Fixed and Word one, Bool and Byte one byte, Unit none.
        let ty = Ty::Tuple(vec![Ty::Text, Ty::Word, Ty::Bool, Ty::Unit, Ty::Byte]);
        assert_eq!(ty.layout().size_in_bytes(8, 8), 16 + 8 + 1 + 1);
        let array = Ty::Array(Box::new(Ty::Tuple(vec![Ty::Text, Ty::Word])), 3);
        assert_eq!(array.layout().size_in_bytes(8, 8), 3 * 24);
    }

    #[test]
    fn names_use_keleusma_syntax() {
        let ty = Ty::Tuple(vec![Ty::Fixed(4), Ty::Array(Box::new(Ty::Text), 2)]);
        assert_eq!(ty.name(), "(Fixed<4>, [Text; 2])");
    }
}
