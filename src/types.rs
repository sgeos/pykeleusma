//! The declared types of a script's entry point, read from the parsed program.
//!
//! A flat Keleusma composite carries no per-field types, so the binding reads them from
//! the declarations, which `keleusma.compile` has in hand as the parsed program.
//!
//! The modelled types are the primitives, unit, tuples, arrays, structs, enums,
//! options and newtypes, which resolve to their underlying type. A generic struct or
//! enum, a nested option, a multiword integer, an untyped parameter or any other form
//! is recorded as [`Unsupported`] with a description, so a module that uses one still
//! compiles and builds a VM, and the error appears only when a value of that type is
//! converted.
//!
//! Layouts come from Keleusma's own [`LayoutDescriptor`], which is also what the
//! compiler uses through its type checker, so field offsets match the compiled code
//! by construction rather than by a copy of its rules.

use std::collections::BTreeMap;

use keleusma::ast::{
    ConstExpr, FunctionCategory, ImportItem, PrimType, Program, TypeDef, TypeExpr,
};
use keleusma::value_layout::{LayoutDescriptor, ScalarKind};

/// One variant of an enum type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    /// The discriminant Keleusma stores in the flat body's leading word.
    pub disc: i64,
    pub fields: Vec<Ty>,
}

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
    Struct {
        name: String,
        fields: Vec<(String, Ty)>,
    },
    Enum {
        name: String,
        variants: Vec<Variant>,
    },
    /// `Option<T>`, which Keleusma lays out as an enum of `None` and `Some(T)`.
    Option(Box<Ty>),
    /// A newtype with a refinement rule, laid out as its underlying type. The script
    /// checks the rule whenever it builds one, so a value may leave the script, but the
    /// host cannot build one, because the rule is script code the binding cannot run.
    Refined {
        name: String,
        inner: Box<Ty>,
    },
}

/// The discriminants Keleusma uses for `Option`, as in its own marshalling.
pub const OPTION_NONE: i64 = 0;
pub const OPTION_SOME: i64 = 1;

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
            Ty::Struct { name, fields } => LayoutDescriptor::Struct {
                type_name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(n, t)| (n.clone(), t.layout()))
                    .collect(),
            },
            Ty::Enum { name, variants } => LayoutDescriptor::Enum {
                type_name: name.clone(),
                variants: variants
                    .iter()
                    .map(|v| (v.name.clone(), v.fields.iter().map(Ty::layout).collect()))
                    .collect(),
            },
            Ty::Refined { inner, .. } => inner.layout(),
            Ty::Option(inner) => LayoutDescriptor::Enum {
                type_name: "Option".to_string(),
                variants: vec![
                    ("None".to_string(), vec![]),
                    ("Some".to_string(), vec![inner.layout()]),
                ],
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
            Ty::Struct { name, .. } | Ty::Enum { name, .. } | Ty::Refined { name, .. } => {
                name.clone()
            }
            Ty::Option(inner) => format!("Option<{}>", inner.name()),
        }
    }
}

/// The deepest chain of named types the resolver follows. Keleusma rejects a type that
/// contains itself, so this bound only stops a malformed program from looping.
const MAX_DEPTH: usize = 32;

/// Resolves type expressions against one program's struct, enum and newtype
/// definitions.
#[derive(Debug, Clone)]
pub struct Resolver {
    default_frac_bits: u8,
    defs: BTreeMap<String, TypeDef>,
}

impl Resolver {
    /// `default_frac_bits` is the target's default for a bare `Fixed`, which the type
    /// checker substitutes the same way.
    pub fn new(program: &Program, default_frac_bits: u8) -> Self {
        let defs = program
            .types
            .iter()
            .map(|def| {
                let name = match def {
                    TypeDef::Struct(s) => s.name.clone(),
                    TypeDef::Enum(e) => e.name.clone(),
                    TypeDef::Newtype(n) => n.name.clone(),
                };
                (name, def.clone())
            })
            .collect();
        Self {
            default_frac_bits,
            defs,
        }
    }

    /// Resolve a declared type expression.
    pub fn resolve(&self, expr: &TypeExpr) -> Declared {
        self.resolve_at(expr, 0)
    }

    fn resolve_at(&self, expr: &TypeExpr, depth: usize) -> Declared {
        if depth > MAX_DEPTH {
            return Err(Unsupported("a type nested too deeply".to_string()));
        }
        let next = depth + 1;
        match expr {
            TypeExpr::Unit(_) => Ok(Ty::Unit),
            TypeExpr::Prim(prim, _) => Ok(match prim {
                PrimType::Bool => Ty::Bool,
                PrimType::Byte => Ty::Byte,
                PrimType::Word => Ty::Word,
                PrimType::Fixed(bits) => Ty::Fixed(bits.unwrap_or(self.default_frac_bits)),
                PrimType::Float => Ty::Float,
                PrimType::Text => Ty::Text,
            }),
            TypeExpr::Tuple(elements, _) => elements
                .iter()
                .map(|e| self.resolve_at(e, next))
                .collect::<Result<Vec<_>, _>>()
                .map(Ty::Tuple),
            TypeExpr::Array(element, length, _) => {
                let element = self.resolve_at(element, next)?;
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
            TypeExpr::Labelled(inner, _, _) => self.resolve_at(inner, next),
            TypeExpr::Option(inner, _) => match self.resolve_at(inner, next)? {
                // `None` could not tell an outer `None` from an inner one in Python.
                Ty::Option(_) => Err(Unsupported(
                    "a nested option, whose `None` would be ambiguous in Python".to_string(),
                )),
                inner => Ok(Ty::Option(Box::new(inner))),
            },
            TypeExpr::Named(name, args, consts, _) => {
                if !args.is_empty() || !consts.is_empty() {
                    return Err(Unsupported(format!("the generic type `{name}`")));
                }
                self.named(name, next)
            }
            TypeExpr::Multiword(_, _, _) => Err(Unsupported("a multiword integer".to_string())),
            #[allow(unreachable_patterns)]
            _ => Err(Unsupported("this type form".to_string())),
        }
    }

    fn named(&self, name: &str, depth: usize) -> Declared {
        let def = self
            .defs
            .get(name)
            .ok_or_else(|| Unsupported(format!("the unknown type `{name}`")))?;
        match def {
            TypeDef::Struct(s) => {
                if !s.type_params.is_empty() || !s.const_params.is_empty() {
                    return Err(Unsupported(format!("the generic struct `{name}`")));
                }
                let fields = s
                    .fields
                    .iter()
                    .map(|f| Ok((f.name.clone(), self.resolve_at(&f.type_expr, depth)?)))
                    .collect::<Result<Vec<_>, Unsupported>>()?;
                Ok(Ty::Struct {
                    name: name.to_string(),
                    fields,
                })
            }
            TypeDef::Enum(e) => {
                if !e.type_params.is_empty() || !e.const_params.is_empty() {
                    return Err(Unsupported(format!("the generic enum `{name}`")));
                }
                let variants = e
                    .variants
                    .iter()
                    .map(|v| {
                        let fields = v
                            .fields
                            .iter()
                            .map(|t| self.resolve_at(t, depth))
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok(Variant {
                            name: v.name.clone(),
                            disc: v.discriminant_value,
                            fields,
                        })
                    })
                    .collect::<Result<Vec<_>, Unsupported>>()?;
                Ok(Ty::Enum {
                    name: name.to_string(),
                    variants,
                })
            }
            // A newtype has its underlying type's representation.
            TypeDef::Newtype(n) => {
                let inner = self.resolve_at(&n.underlying, depth)?;
                Ok(match n.refinement {
                    Some(_) => Ty::Refined {
                        name: name.to_string(),
                        inner: Box::new(inner),
                    },
                    None => inner,
                })
            }
        }
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
    /// The type of the value `resume` passes in.
    ///
    /// Keleusma's verifier records that a stream resumes with its single parameter's
    /// type, and a `yield main` was observed to behave the same way. The VM does not
    /// check a resumed value's type at all, which was also observed, so this
    /// declaration is the only check and is unsupported wherever it is not certain.
    pub resume: Declared,
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
        let resolver = Resolver::new(program, default_frac_bits);
        let mut heads = program.functions.iter().filter(|f| f.name == "main");
        let first = heads.next()?;
        let signature_of = |f: &keleusma::ast::FunctionDef| {
            let category = match f.category {
                FunctionCategory::Fn => Category::Fn,
                FunctionCategory::Yield => Category::Yield,
                FunctionCategory::Loop => Category::Loop,
            };
            let params: Vec<Declared> = f
                .params
                .iter()
                .map(|p| match &p.type_expr {
                    Some(expr) => resolver.resolve(expr),
                    None => Err(Unsupported(
                        "a parameter without a declared type".to_string(),
                    )),
                })
                .collect();
            let resume = match (category, params.as_slice()) {
                (Category::Fn, _) => Err(Unsupported(
                    "no resume value, because a `fn main` cannot be resumed".to_string(),
                )),
                (_, [only]) => only.clone(),
                (_, others) => Err(Unsupported(format!(
                    "no single resume type, because `main` has {} parameters",
                    others.len()
                ))),
            };
            EntrySignature {
                category,
                params,
                ret: resolver.resolve(&f.return_type),
                resume,
            }
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
                resume: mixed(),
            });
        }
        Some(signature)
    }
}

/// What a `use` line declares about one native.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDecl {
    /// The parameter and return types from `use host::name(T1, T2) -> R`, or `None`
    /// for a bare `use host::name`, whose values convert only as scalars.
    pub signature: Option<(Vec<Declared>, Declared)>,
    /// Whether the line is `use external ...`, which 0.1.0 does not support.
    pub external: bool,
}

/// The natives the program declares by name, keyed by the full name Keleusma uses,
/// such as `host::double`. A wildcard import names no native, so its natives are
/// absent here and convert as untyped.
pub fn native_decls(program: &Program, default_frac_bits: u8) -> BTreeMap<String, NativeDecl> {
    let resolver = Resolver::new(program, default_frac_bits);
    program
        .uses
        .iter()
        .filter_map(|decl| {
            let ImportItem::Name(name) = &decl.import else {
                return None;
            };
            let mut full = decl.path.join("::");
            if !full.is_empty() {
                full.push_str("::");
            }
            full.push_str(name);
            let signature = decl.signature.as_ref().map(|sig| {
                let params = sig.params.iter().map(|p| resolver.resolve(p)).collect();
                (params, resolver.resolve(&sig.return_type))
            });
            Some((
                full,
                NativeDecl {
                    signature,
                    external: decl.is_external,
                },
            ))
        })
        .collect()
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
        assert_eq!(s.resume, s.params[0]);
    }

    #[test]
    fn only_a_single_parameter_coroutine_has_a_resume_type() {
        assert_eq!(
            signature("yield main(e: Text) -> Word { 1 }").resume,
            Ok(Ty::Text)
        );
        assert!(signature("fn main(e: Word) -> Word { e }").resume.is_err());
        assert!(
            signature("yield main(a: Word, b: Word) -> Word { a }")
                .resume
                .is_err()
        );
    }

    #[test]
    fn a_named_type_is_unsupported_but_does_not_fail_the_signature() {
        let s = signature("struct P<T> { x: T }\nfn main(p: P<Word>, n: Word) -> Word { n }");
        assert!(matches!(&s.params[0], Err(Unsupported(m)) if m.contains("`P`")));
        assert_eq!(s.params[1], Ok(Ty::Word));
        assert_eq!(s.ret, Ok(Ty::Word));
    }

    #[test]
    fn native_declarations_resolve() {
        let program = parse(
            &tokenize(
                "use host::double(Word) -> Word\nuse host::raw\nuse external io::send\n\
                 fn main() -> Word { host::double(1) }",
            )
            .expect("lex"),
        )
        .expect("parse");
        let decls = native_decls(&program, DEFAULT);
        assert_eq!(
            decls["host::double"].signature,
            Some((vec![Ok(Ty::Word)], Ok(Ty::Word)))
        );
        assert_eq!(decls["host::raw"].signature, None);
        assert!(decls["io::send"].external);
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
