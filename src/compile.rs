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

use keleusma::bytecode::Module as KeleusmaModule;
use keleusma::compiler::compile as keleusma_compile;
use keleusma::lexer::tokenize;
use keleusma::parser::parse;
use keleusma::token::Span;
use pyo3::prelude::*;

pyo3::import_exception!(keleusma._errors, LexError);
pyo3::import_exception!(keleusma._errors, ParseError);
pyo3::import_exception!(keleusma._errors, CompileError);

/// A compiled Keleusma module.
///
/// Immutable from Python. `frozen` is sound because `keleusma::bytecode::Module` is
/// `Send` and `Sync`, which the compiler checks for every `#[pyclass(frozen)]`.
#[pyclass(frozen, module = "keleusma._keleusma", name = "Module")]
pub struct Module {
    inner: KeleusmaModule,
}

impl Module {
    /// The compiled module, for constructing a VM from it.
    pub fn inner(&self) -> &KeleusmaModule {
        &self.inner
    }
}

#[pymethods]
impl Module {
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

fn run_pipeline(source: &str) -> Result<KeleusmaModule, StageError> {
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
    keleusma_compile(&program).map_err(|e| StageError {
        stage: Stage::Compile,
        message: e.message,
        span: e.span,
    })
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

/// Compile source text, for callers inside the extension.
///
/// Releases the GIL while Keleusma compiles, because compilation time is not bounded
/// by Keleusma's guarantees and other Python threads should keep running meanwhile.
pub fn compile_source(py: Python<'_>, source: &str) -> PyResult<KeleusmaModule> {
    py.detach(|| run_pipeline(source))
        .map_err(|error| to_py_err(source, error))
}

/// Compile Keleusma source text into a module.
///
/// Raises `keleusma.LexError`, `keleusma.ParseError` or `keleusma.CompileError`, each a
/// `keleusma.SourceError` carrying `msg`, `lineno` and `colno`.
#[pyfunction]
pub fn compile(py: Python<'_>, source: &str) -> PyResult<Module> {
    compile_source(py, source).map(|inner| Module { inner })
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
