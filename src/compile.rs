//! Roadmap task 2. `keleusma.compile(source)` and the `Module` it returns.
//!
//! Compilation runs Keleusma's own pipeline, `lexer::tokenize`, then `parser::parse`,
//! then `compiler::compile`. Each stage's error becomes the matching Python exception
//! from `keleusma._errors`, carrying the message and the 1-based line and column.
//!
//! # Columns
//!
//! Keleusma's `Span::column` counts bytes from the start of the line. Python counts
//! characters, so a column after any non-ASCII character would point at the wrong
//! place. [`char_column`] converts using the source text and the span's byte offset,
//! and falls back to Keleusma's byte column if the span does not fit the source,
//! rather than failing while reporting another failure.
//!
//! # Unknown locations
//!
//! `Span` derives `Default`, and the compiler builds some synthesised types with the
//! default span, whose line is 0. A line of 0 therefore means that no location is
//! known, and the exception reports `None` for both line and column.

use std::collections::BTreeMap;

use keleusma::ast::Program;
use keleusma::bytecode::Module as KeleusmaModule;
use keleusma::compiler::compile as keleusma_compile;
use keleusma::lexer::tokenize;
use keleusma::parser::parse;
use keleusma::target::Target;
use keleusma::token::Span;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::convert::Widths;
use crate::crypto;
use crate::types::{EntrySignature, NativeDecl, native_decls};
use crate::yield_check;
use keleusma::wire_format;

pyo3::import_exception!(keleusma._errors, LexError);
pyo3::import_exception!(keleusma._errors, ParseError);
pyo3::import_exception!(keleusma._errors, CompileError);
pyo3::import_exception!(keleusma._errors, LoadError);
pyo3::import_exception!(keleusma._errors, VerifyError);

/// A compiled Keleusma module, with the declared types at its entry point.
///
/// Immutable from Python. `frozen` is sound because `keleusma::bytecode::Module` is
/// `Send` and `Sync`, which the compiler checks for every `#[pyclass(frozen)]`.
#[pyclass(frozen, module = "keleusma._keleusma", name = "Module")]
pub struct Module {
    inner: KeleusmaModule,
    boundary: Boundary,
    /// For a module loaded from signed bytes, those bytes and the keys they verified
    /// against, so a VM is built through Keleusma's signed loader and verifies again.
    signed: Option<SignedImage>,
}

/// How a signed module was verified, and the keys it was verified against.
#[derive(Debug, Clone)]
pub struct SignedImage {
    /// The bytes as they were loaded, whose signature a VM's trust matrix verifies
    /// again at a hot swap. For an encrypted module these are the encrypted bytes,
    /// because the signature covers the encrypted form, and Keleusma verifies that
    /// form without the decryption key.
    pub bytes: Vec<u8>,
    /// Whether `bytes` are encrypted. The decoded module then came from decrypting
    /// them, which authenticated the ciphertext, and carries its signature flag
    /// cleared, as in Keleusma's own encrypted loader.
    pub encrypted: bool,
    pub keys: Vec<ed25519_dalek::VerifyingKey>,
}

/// What value conversion needs to know about a module, kept from compilation
/// because the compiled module no longer holds the declared types.
#[derive(Debug, Clone)]
pub struct Boundary {
    /// The types of `main`, or `None` if the program has no `main`.
    pub signature: Option<EntrySignature>,
    /// The scalar widths flat bodies are packed with.
    pub widths: Widths,
    /// What the program's `use` lines declare about each named native.
    pub natives: BTreeMap<String, NativeDecl>,
}

impl Module {
    /// The compiled module, for constructing a VM from it.
    pub fn inner(&self) -> &KeleusmaModule {
        &self.inner
    }

    /// The entry point's declared types and the module's widths.
    pub fn boundary(&self) -> &Boundary {
        &self.boundary
    }

    /// The signed image, for a module loaded from signed bytes.
    pub fn signed(&self) -> Option<&SignedImage> {
        self.signed.as_ref()
    }

    /// Refuse a module whose source is declared `signed` but which was not loaded from
    /// signed bytes, since Keleusma builds such a module only through its signed
    /// loader, and its own message names Rust functions a Python host cannot call.
    pub fn check_signature_route(&self) -> PyResult<()> {
        let declared = self.inner.flags & wire_format::FLAG_REQUIRES_SIGNATURE != 0;
        if declared && self.signed.is_none() {
            return Err(VerifyError::new_err(
                "the module is declared `signed`, so it runs only from signed bytes; \
                 sign it with `Module.to_signed_bytes` and load the result with \
                 `keleusma.load_module` and the matching `verifying_keys`",
            ));
        }
        Ok(())
    }
}

#[pymethods]
impl Module {
    /// The module as Keleusma's wire format, the bytes a `.kel.bin` file holds.
    ///
    /// The bytes carry no source, so a module loaded back from them with
    /// `keleusma.load_module` converts values without declared types.
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = self
            .inner
            .to_bytes()
            .map_err(|e| LoadError::new_err(format!("{e:?}")))?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// The module as signed wire-format bytes, signed with the Ed25519 key whose 32-byte
    /// seed is `signing_seed`, as the book's chapter 26 describes.
    ///
    /// With `recipient_key`, an X25519 public key, the body is also encrypted to that
    /// recipient, using a fresh ephemeral key from the operating system's random
    /// source, so only the holder of the matching private key can load it.
    #[pyo3(signature = (signing_seed, *, recipient_key = None))]
    fn to_signed_bytes<'py>(
        &self,
        py: Python<'py>,
        signing_seed: &[u8],
        recipient_key: Option<&[u8]>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let key = crypto::signing_key(signing_seed)?;
        let bytes = match recipient_key {
            None => wire_format::module_to_signed_wire_bytes(&self.inner, &key),
            Some(recipient) => {
                let recipient = crypto::x25519_key(recipient, "a recipient key")?;
                let ephemeral: Vec<u8> = py
                    .import("os")?
                    .getattr("urandom")?
                    .call1((32,))?
                    .extract()?;
                let ephemeral = crypto::x25519_key(&ephemeral, "an ephemeral key")?;
                wire_format::module_to_encrypted_signed_wire_bytes(
                    &self.inner,
                    &key,
                    recipient,
                    ephemeral,
                )
            }
        }
        .map_err(|e| LoadError::new_err(format!("{e:?}")))?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Whether the module came from signed bytes.
    #[getter]
    fn is_signed(&self) -> bool {
        self.signed.is_some()
    }

    /// Whether the module came from source, so values convert by declared types.
    #[getter]
    fn has_declared_types(&self) -> bool {
        self.boundary.signature.is_some()
    }

    fn __repr__(&self) -> String {
        format!(
            "<keleusma.Module with {} chunk(s) and {} native(s)>",
            self.inner.chunks.len(),
            self.inner.native_names.len()
        )
    }
}

/// Which pipeline stage rejected the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Lex,
    Parse,
    Compile,
}

/// A stage failure, held without the GIL until it is turned into a Python exception.
#[derive(Debug)]
struct StageError {
    stage: Stage,
    message: String,
    span: Span,
}

/// The 1-based character column of `span` within `source`, or `None` if the span
/// carries no location.
///
/// Uses the byte offset `span.start` and the byte column to find the start of the
/// line, then counts the characters between the two. If either offset does not land
/// on a character boundary inside `source`, the byte column is returned unchanged.
fn char_column(source: &str, span: Span) -> Option<u32> {
    if span.line == 0 || span.column == 0 {
        return None;
    }
    let byte_column = usize::try_from(span.column - 1).ok();
    let converted = byte_column.and_then(|offset| {
        let line_start = span.start.checked_sub(offset)?;
        let prefix = source.get(line_start..span.start)?;
        u32::try_from(prefix.chars().count()).ok()?.checked_add(1)
    });
    Some(converted.unwrap_or(span.column))
}

/// The location as Python sees it, a 1-based line and character column, or two
/// `None`s when Keleusma reports no location.
fn location(source: &str, span: Span) -> (Option<u32>, Option<u32>) {
    match char_column(source, span) {
        Some(column) => (Some(span.line), Some(column)),
        None => (None, None),
    }
}

fn run_pipeline(source: &str) -> Result<(KeleusmaModule, Program), StageError> {
    let tokens = tokenize(source).map_err(|e| StageError {
        stage: Stage::Lex,
        message: e.message,
        span: e.span,
    })?;
    let program = parse(&tokens).map_err(|e| StageError {
        stage: Stage::Parse,
        message: e.message,
        span: e.span,
    })?;
    let module = keleusma_compile(&program).map_err(|e| StageError {
        stage: Stage::Compile,
        message: e.message,
        span: e.span,
    })?;
    // Keleusma accepts some mistyped yields, which could crash a later read, so the
    // binding checks them itself. See `yield_check` for the reasoning and the limits.
    match yield_check::check(&program, Target::host()) {
        Ok(None) => Ok((module, program)),
        Ok(Some(mismatch)) => Err(StageError {
            stage: Stage::Compile,
            message: mismatch.message,
            span: mismatch.span,
        }),
        // The re-check repeats steps that just succeeded inside the compiler, so a
        // failure means the two have diverged. Failing closed keeps the guarantee.
        Err(message) => Err(StageError {
            stage: Stage::Compile,
            message: format!("the binding could not re-check this program's yields: {message}"),
            span: Span::default(),
        }),
    }
}

fn to_py_err(source: &str, error: StageError) -> PyErr {
    let (line, column) = location(source, error.span);
    let args = (error.message, line, column);
    match error.stage {
        Stage::Lex => LexError::new_err(args),
        Stage::Parse => ParseError::new_err(args),
        Stage::Compile => CompileError::new_err(args),
    }
}

/// Compile Keleusma source text into a module.
///
/// Raises `keleusma.LexError`, `keleusma.ParseError` or `keleusma.CompileError`, each a
/// `keleusma.SourceError` carrying `msg`, `lineno` and `colno`.
///
/// Releases the GIL while Keleusma compiles, because compilation time is not bounded
/// by Keleusma's guarantees and other Python threads should keep running meanwhile.
#[pyfunction]
pub fn compile(py: Python<'_>, source: &str) -> PyResult<Module> {
    let (inner, program) = py
        .detach(|| run_pipeline(source))
        .map_err(|error| to_py_err(source, error))?;
    // `keleusma::compiler::compile` targets the host, so a bare `Fixed` takes the
    // host's default fraction bits, as the type checker substitutes them.
    let default_frac_bits = Target::host().fixed_default_frac_bits();
    let boundary = Boundary {
        signature: EntrySignature::from_program(&program, default_frac_bits),
        widths: Widths::of(&inner),
        natives: native_decls(&program, default_frac_bits),
    };
    Ok(Module {
        inner,
        boundary,
        signed: None,
    })
}

/// Load a module from Keleusma's wire format, such as a `.kel.bin` file.
///
/// Verification of the module itself happens when a VM is built from it, exactly as
/// for a compiled module. The bytes carry no source, so the binding knows no declared
/// types for the entry point or the natives, and converts their values as untyped
/// scalars, refusing composites and `Fixed`.
///
/// A signed module, the book's chapters 26 and 36, loads only with `verifying_keys`,
/// 32-byte Ed25519 public keys, and only if its signature verifies against one of
/// them. An encrypted module also needs `decryption_key`, the recipient's 32-byte
/// X25519 private key. Giving keys for a module that is not signed is refused, as
/// Keleusma refuses it, so a host that expects signed code cannot be handed unsigned
/// code. Raises `keleusma.LoadError` for malformed bytes and `keleusma.VerifyError`
/// for a failed signature or a missing key.
///
/// # Trust boundary
///
/// Bytecode is code. Its checksum detects accidental corruption, not tampering. The
/// bytes are decoded by Keleusma's own decoder and checked by its verifier, and the
/// binding has not established that either is robust against crafted input. Keleusma
/// 0.2.2 has a confirmed crash in its decoding of values from a program its compiler
/// accepts, and no fuzzing of this function has been done. Unsigned bytes should
/// therefore come only from a source the host trusts as it would trust the script's
/// source. A signature establishes who produced the bytes, not that they are safe, so
/// it protects only to the extent that the holders of the trusted keys are trusted.
#[pyfunction]
#[pyo3(signature = (data, *, verifying_keys = None, decryption_key = None))]
pub fn load_module(
    data: &[u8],
    verifying_keys: Option<Vec<Vec<u8>>>,
    decryption_key: Option<&[u8]>,
) -> PyResult<Module> {
    let keys = verifying_keys
        .unwrap_or_default()
        .iter()
        .map(|k| crypto::verifying_key(k))
        .collect::<PyResult<Vec<_>>>()?;
    let (bytes, signed) = if wire_format::header_requires_encryption(data) {
        let private = decryption_key.ok_or_else(|| {
            VerifyError::new_err("the module is encrypted; pass `decryption_key`")
        })?;
        let private = crypto::x25519_key(private, "a decryption key")?;
        if keys.is_empty() {
            return Err(VerifyError::new_err(
                "the module is signed; pass the `verifying_keys` it may be signed with",
            ));
        }
        // Verifies the signature over the encrypted form, then decrypts.
        let plain = wire_format::decrypt_encrypted_signed_to_signed_bytes(data, &keys, private)
            .map_err(|e| VerifyError::new_err(format!("{e:?}")))?;
        let image = SignedImage {
            bytes: data.to_vec(),
            encrypted: true,
            keys,
        };
        (plain, Some(image))
    } else if decryption_key.is_some() {
        // A host that passes a decryption key expects a confidential module, so a
        // plaintext one is refused rather than accepted silently.
        return Err(VerifyError::new_err(
            "a decryption key was given, but the module is not encrypted",
        ));
    } else if wire_format::header_requires_signature(data) {
        if keys.is_empty() {
            return Err(VerifyError::new_err(
                "the module is signed; pass the `verifying_keys` it may be signed with",
            ));
        }
        wire_format::verify_module_signature(data, &keys)
            .map_err(|e| VerifyError::new_err(format!("{e:?}")))?;
        let image = SignedImage {
            bytes: data.to_vec(),
            encrypted: false,
            keys,
        };
        (data.to_vec(), Some(image))
    } else if !keys.is_empty() {
        return Err(VerifyError::new_err(
            "verifying keys were given, but the module is not signed",
        ));
    } else {
        (data.to_vec(), None)
    };
    let mut inner =
        KeleusmaModule::from_bytes(&bytes).map_err(|e| LoadError::new_err(format!("{e:?}")))?;
    // A decrypted module was verified during decryption, so it is built like an
    // unsigned one, as Keleusma's own encrypted loader does.
    if matches!(
        &signed,
        Some(SignedImage {
            encrypted: true,
            ..
        })
    ) {
        inner.flags &= !wire_format::FLAG_REQUIRES_SIGNATURE;
    }
    let boundary = Boundary {
        signature: None,
        widths: Widths::of(&inner),
        natives: BTreeMap::new(),
    };
    Ok(Module {
        inner,
        boundary,
        signed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize, line: u32, column: u32) -> Span {
        Span {
            start,
            end: start + 1,
            line,
            column,
        }
    }

    #[test]
    fn an_ascii_line_keeps_its_column() {
        let source = "abc def";
        assert_eq!(char_column(source, span(4, 1, 5)), Some(5));
    }

    #[test]
    fn a_column_after_multibyte_characters_counts_characters() {
        // "日本" is six bytes and two characters, so the space after it is at byte
        // column 7 and character column 3, and the 'x' at byte 7 is column 4.
        let source = "日本 x";
        let start = source.find('x').expect("x");
        assert_eq!(start, 7);
        assert_eq!(char_column(source, span(start, 1, 8)), Some(4));
    }

    #[test]
    fn a_column_on_a_later_line_is_counted_from_that_line() {
        let source = "é\nab日x";
        let start = source.find('x').expect("x");
        let line_start = source.find('\n').expect("newline") + 1;
        let byte_column = u32::try_from(start - line_start + 1).expect("fits");
        assert_eq!(char_column(source, span(start, 2, byte_column)), Some(4));
    }

    #[test]
    fn line_zero_means_no_location() {
        assert_eq!(char_column("abc", Span::default()), None);
        assert_eq!(location("abc", Span::default()), (None, None));
    }

    #[test]
    fn a_span_that_does_not_fit_the_source_falls_back_to_the_byte_column() {
        // Past the end of the source.
        assert_eq!(char_column("abc", span(99, 1, 3)), Some(3));
        // A column larger than the offset, so the line start would be negative.
        assert_eq!(char_column("abc", span(1, 1, 9)), Some(9));
        // A line start inside a multibyte character.
        let source = "日x";
        assert_eq!(char_column(source, span(3, 1, 2)), Some(2));
    }
}
