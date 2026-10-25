//! Roadmap task 4. Conversion between Python objects and Keleusma values, by type.
//!
//! | Keleusma | Python | Into Keleusma |
//! |---|---|---|
//! | `()` | `None` | Only `None` |
//! | `bool` | `bool` | Only `bool` |
//! | `Byte` | `int` | `int` in 0 to 255, else `OverflowError` |
//! | `Word` | `int` | `int` in the signed 64-bit range, else `OverflowError` |
//! | `Fixed<N>` | `keleusma.Fixed` | Any exact number, else `ValueError` or `OverflowError` |
//! | `Float` | `float` | `float` or `int` |
//! | `Text` | `str` | Only `str` |
//! | Tuple | `tuple` | A `tuple` of the declared length |
//! | Array | `list` | A `list` or `tuple` of the declared length |
//!
//! A `bool` is never accepted where a number is declared, although Python treats it
//! as an `int`, because passing `True` as a count is far more often a mistake than an
//! intent. A type mismatch raises `TypeError`.
//!
//! # Reading values out
//!
//! Keleusma 0.2.2 stores a tuple or array of scalars and text as a flat byte body in
//! the VM's arena, with no per-field types. The body is read by the declared type, at
//! offsets from Keleusma's own `LayoutDescriptor`, and its length is checked against
//! the declared layout first. A value that does not match its declared type, which
//! Keleusma's type checker can let through, raises `keleusma.VMError` rather than
//! being misread. `TupleBody::elements()` panics on a flat body, so it is never called.
//!
//! A value is converted immediately after the call that produced it, because a flat
//! body is valid only until the next call or resume on the same VM.

use keleusma::Arena;
use keleusma::bytecode::{ArrayBody, TupleBody, Value};
use keleusma::flat_value::FlatComposite;
use keleusma::marshall::{KeleusmaType, RefContext};
use keleusma::value_layout::LayoutDescriptor;
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyOverflowError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyList, PyString, PyTuple};

use crate::types::Ty;
use crate::vm::VMError;

/// The module's scalar widths in bytes, which flat bodies are packed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Widths {
    pub word_bytes: usize,
    pub float_bytes: usize,
}

impl Widths {
    /// Widths from a module header, which stores each as the base-2 logarithm of a
    /// bit count.
    pub fn of(module: &keleusma::bytecode::Module) -> Self {
        let bytes = |log2: u8| (1usize << log2) / 8;
        Self {
            word_bytes: bytes(module.word_bits_log2),
            float_bytes: bytes(module.float_bits_log2),
        }
    }
}

fn type_error(expected: &Ty, obj: &Bound<'_, PyAny>) -> PyErr {
    let got = obj
        .get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "an unknown type".to_string());
    PyTypeError::new_err(format!("expected {}, got {got}", expected.name()))
}

/// An `int` that is not a `bool`.
fn is_integer(obj: &Bound<'_, PyAny>) -> bool {
    obj.is_instance_of::<PyInt>() && !obj.is_instance_of::<PyBool>()
}

fn elements<'py>(
    items: impl ExactSizeIterator<Item = Bound<'py, PyAny>>,
    element_types: &[Ty],
) -> PyResult<Vec<Value>> {
    items
        .zip(element_types)
        .map(|(item, ty)| to_value(&item, ty))
        .collect()
}

/// Convert a Python object to a Keleusma value of the declared type.
pub fn to_value(obj: &Bound<'_, PyAny>, ty: &Ty) -> PyResult<Value> {
    match ty {
        Ty::Unit if obj.is_none() => Ok(Value::Unit),
        Ty::Bool if obj.is_instance_of::<PyBool>() => Ok(Value::Bool(obj.is_truthy()?)),
        Ty::Word if is_integer(obj) => Ok(Value::Int(obj.extract::<i64>()?)),
        Ty::Byte if is_integer(obj) => {
            let out_of_range =
                || PyOverflowError::new_err(format!("{obj} is out of range for Byte, 0 to 255"));
            let n = obj.extract::<i64>().map_err(|_| out_of_range())?;
            u8::try_from(n).map(Value::Byte).map_err(|_| out_of_range())
        }
        Ty::Fixed(bits) => {
            let raw = obj
                .py()
                .import("keleusma._fixed")?
                .getattr("raw_for")?
                .call1((obj, *bits))?
                .extract::<i64>()?;
            Ok(Value::Fixed(raw))
        }
        Ty::Float if obj.is_instance_of::<PyFloat>() || is_integer(obj) => {
            Ok(Value::Float(obj.extract::<f64>()?))
        }
        Ty::Text if obj.is_instance_of::<PyString>() => {
            Ok(Value::StaticStr(obj.extract::<String>()?))
        }
        Ty::Tuple(types) => {
            let tuple = obj.cast::<PyTuple>().map_err(|_| type_error(ty, obj))?;
            if tuple.len() != types.len() {
                return Err(PyTypeError::new_err(format!(
                    "expected {}, got a tuple of length {}",
                    ty.name(),
                    tuple.len()
                )));
            }
            Ok(Value::tuple(elements(tuple.iter(), types)?))
        }
        Ty::Array(element, count) => {
            let items: Vec<Bound<'_, PyAny>> = if let Ok(list) = obj.cast::<PyList>() {
                list.iter().collect()
            } else if let Ok(tuple) = obj.cast::<PyTuple>() {
                tuple.iter().collect()
            } else {
                return Err(type_error(ty, obj));
            };
            if items.len() != *count {
                return Err(PyTypeError::new_err(format!(
                    "expected {}, got a sequence of length {}",
                    ty.name(),
                    items.len()
                )));
            }
            let types = vec![(**element).clone(); *count];
            Ok(Value::array(elements(items.into_iter(), &types)?))
        }
        _ => Err(type_error(ty, obj)),
    }
}

/// What reading a value out needs from the VM that produced it.
pub struct ReadContext<'a> {
    pub arena: &'a Arena,
    pub widths: Widths,
}

fn mismatch(ty: &Ty, found: &str) -> PyErr {
    VMError::new_err(format!(
        "the script produced {found}, where its declared type is {}",
        ty.name()
    ))
}

fn stale() -> PyErr {
    VMError::new_err("the value is no longer valid, because the VM has run since")
}

fn fixed(py: Python<'_>, raw: i64, bits: u8) -> PyResult<Bound<'_, PyAny>> {
    py.import("keleusma._fixed")?
        .getattr("Fixed")?
        .call1((raw, bits))
}

/// Convert a Keleusma value of the declared type to a Python object.
pub fn to_py<'py>(
    py: Python<'py>,
    value: &Value,
    ty: &Ty,
    ctx: &ReadContext<'_>,
) -> PyResult<Bound<'py, PyAny>> {
    match (ty, value) {
        (Ty::Unit, Value::Unit) => Ok(py.None().into_bound(py)),
        (Ty::Bool, Value::Bool(b)) => b.into_bound_py_any(py),
        (Ty::Byte, Value::Byte(b)) => b.into_bound_py_any(py),
        (Ty::Word, Value::Int(n)) => n.into_bound_py_any(py),
        (Ty::Fixed(bits), Value::Fixed(raw)) => fixed(py, *raw, *bits),
        (Ty::Float, Value::Float(f)) => f.into_bound_py_any(py),
        (Ty::Text, Value::StaticStr(s)) => s.into_bound_py_any(py),
        (Ty::Text, Value::KStr(k)) => k.get(ctx.arena).map_err(|_| stale())?.into_bound_py_any(py),
        (Ty::Tuple(types), Value::Tuple(TupleBody::Boxed(items))) => {
            if items.len() != types.len() {
                return Err(mismatch(ty, &format!("a tuple of length {}", items.len())));
            }
            let converted = items
                .iter()
                .zip(types)
                .map(|(item, t)| to_py(py, item, t, ctx))
                .collect::<PyResult<Vec<_>>>()?;
            Ok(PyTuple::new(py, converted)?.into_any())
        }
        (Ty::Array(element, count), Value::Array(ArrayBody::Boxed(items))) => {
            if items.len() != *count {
                return Err(mismatch(ty, &format!("an array of length {}", items.len())));
            }
            let converted = items
                .iter()
                .map(|item| to_py(py, item, element, ctx))
                .collect::<PyResult<Vec<_>>>()?;
            Ok(PyList::new(py, converted)?.into_any())
        }
        (Ty::Tuple(_), Value::Tuple(TupleBody::Flat(body)))
        | (Ty::Array(..), Value::Array(ArrayBody::Flat(body))) => flat_body(py, body, ty, ctx),
        (_, other) => Err(mismatch(ty, &format!("{other:?}"))),
    }
}

fn flat_body<'py>(
    py: Python<'py>,
    body: &FlatComposite,
    ty: &Ty,
    ctx: &ReadContext<'_>,
) -> PyResult<Bound<'py, PyAny>> {
    let bytes = body.resolve(ctx.arena).map_err(|_| stale())?;
    read_flat(py, bytes, ty, ctx, body.ref_epoch())
}

/// Read a value of type `ty` from exactly its flat bytes.
fn read_flat<'py>(
    py: Python<'py>,
    bytes: &[u8],
    ty: &Ty,
    ctx: &ReadContext<'_>,
    epoch: u64,
) -> PyResult<Bound<'py, PyAny>> {
    let Widths {
        word_bytes,
        float_bytes,
    } = ctx.widths;
    let layout = ty.layout();
    let expected = layout.size_in_bytes(word_bytes, float_bytes);
    if bytes.len() != expected {
        return Err(mismatch(
            ty,
            &format!("a body of {} bytes, not {expected}", bytes.len()),
        ));
    }
    match ty {
        Ty::Text => {
            let refs = RefContext {
                arena: ctx.arena,
                opaques: &[],
                word_bytes,
                float_bytes,
                ref_epoch: epoch,
            };
            let text = <String as KeleusmaType<i64, f64>>::from_flat_bytes_ctx(
                bytes,
                word_bytes,
                float_bytes,
                &refs,
            )
            .map_err(|e| VMError::new_err(format!("{e:?}")))?;
            text.into_bound_py_any(py)
        }
        Ty::Tuple(types) => {
            let items = fields(py, bytes, &layout, types.iter(), ctx, epoch)?;
            Ok(PyTuple::new(py, items)?.into_any())
        }
        Ty::Array(element, count) => {
            let items = fields(
                py,
                bytes,
                &layout,
                std::iter::repeat_n(element.as_ref(), *count),
                ctx,
                epoch,
            )?;
            Ok(PyList::new(py, items)?.into_any())
        }
        scalar => {
            let kind = match layout {
                LayoutDescriptor::Scalar(kind) => kind,
                _ => return Err(mismatch(ty, "a composite layout for a scalar")),
            };
            let value = Value::read_scalar_le(bytes, 0, kind, word_bytes, float_bytes)
                .map_err(|e| VMError::new_err(format!("{e:?}")))?;
            to_py(py, &value, scalar, ctx)
        }
    }
}

/// Read each field of a flat composite at Keleusma's offsets.
fn fields<'a, 'py>(
    py: Python<'py>,
    bytes: &[u8],
    layout: &LayoutDescriptor,
    types: impl Iterator<Item = &'a Ty>,
    ctx: &ReadContext<'_>,
    epoch: u64,
) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let Widths {
        word_bytes,
        float_bytes,
    } = ctx.widths;
    types
        .enumerate()
        .map(|(index, field)| {
            let offset = layout
                .field_offset(index, word_bytes, float_bytes)
                .ok_or_else(|| mismatch(field, "a field outside its composite"))?;
            let size = field.layout().size_in_bytes(word_bytes, float_bytes);
            let slice = offset
                .checked_add(size)
                .and_then(|end| bytes.get(offset..end))
                .ok_or_else(|| mismatch(field, "a field past the end of its composite"))?;
            read_flat(py, slice, field, ctx, epoch)
        })
        .collect()
}

/// Convert a value with no declared type, as a bare `use host::name` gives a native's
/// arguments. Scalars carry their own tags and convert. A `Fixed` has no fraction-bit
/// count without a type, and a composite has no field types, so both need a declared
/// signature, `use host::name(T1, T2) -> R`, and raise `TypeError` otherwise.
pub fn to_py_untyped<'py>(
    py: Python<'py>,
    value: &Value,
    ctx: &ReadContext<'_>,
) -> PyResult<Bound<'py, PyAny>> {
    match value {
        Value::Unit => Ok(py.None().into_bound(py)),
        Value::Bool(b) => b.into_bound_py_any(py),
        Value::Byte(b) => b.into_bound_py_any(py),
        Value::Int(n) => n.into_bound_py_any(py),
        Value::Float(f) => f.into_bound_py_any(py),
        Value::StaticStr(s) => s.into_bound_py_any(py),
        Value::KStr(k) => k.get(ctx.arena).map_err(|_| stale())?.into_bound_py_any(py),
        other => Err(PyTypeError::new_err(format!(
            "a native without a declared signature received {}, which needs one; \
             declare it as `use host::name(T1, T2) -> R`",
            kind_name(other)
        ))),
    }
}

/// Convert a Python result with no declared type, the counterpart of
/// [`to_py_untyped`]. An `int` becomes a `Word`, never a `Byte`, and a `keleusma.Fixed`
/// carries its own representation.
pub fn to_value_untyped(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Unit);
    }
    if obj.is_instance_of::<PyBool>() {
        return Ok(Value::Bool(obj.is_truthy()?));
    }
    if obj.is_instance_of::<PyInt>() {
        return Ok(Value::Int(obj.extract::<i64>()?));
    }
    if obj.is_instance_of::<PyFloat>() {
        return Ok(Value::Float(obj.extract::<f64>()?));
    }
    if obj.is_instance_of::<PyString>() {
        return Ok(Value::StaticStr(obj.extract::<String>()?));
    }
    let fixed = obj.py().import("keleusma._fixed")?.getattr("Fixed")?;
    if obj.is_instance(&fixed)? {
        return Ok(Value::Fixed(obj.getattr("raw")?.extract::<i64>()?));
    }
    let got = obj
        .get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "an unknown type".to_string());
    Err(PyTypeError::new_err(format!(
        "a native without a declared signature returned {got}, which needs one; \
         declare it as `use host::name(T1, T2) -> R`"
    )))
}

fn kind_name(value: &Value) -> &'static str {
    match value {
        Value::Fixed(_) => "a Fixed value",
        Value::Tuple(_) => "a tuple",
        Value::Array(_) => "an array",
        Value::Struct(_) => "a struct",
        Value::Enum(_) => "an enum value",
        _ => "a value of an unsupported kind",
    }
}
