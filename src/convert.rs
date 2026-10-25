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

use std::sync::Arc;

use keleusma::Arena;
use keleusma::bytecode::{ArrayBody, EnumBody, StructBody, TupleBody, Value};
use keleusma::flat_value::FlatComposite;
use keleusma::opaque::HostOpaque;
use keleusma::value_layout::LayoutDescriptor;
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyOverflowError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyMapping, PyString, PyTuple};

use crate::types::{OPTION_NONE, OPTION_SOME, OutOnly, Ty, Variant};
use crate::vm::VMError;

pub use crate::compat::Widths;

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
        Ty::Struct { name, fields } => struct_value(obj, ty, name, fields),
        Ty::Opaque(name) => Ok(crate::opaque::to_value(obj, name)),
        Ty::Multiword { words, frac_bits } => {
            let words: Vec<i64> = obj
                .py()
                .import("keleusma._multiword")?
                .getattr("to_words")?
                .call1((obj, *words, *frac_bits))?
                .extract()?;
            Ok(Value::array(words.into_iter().map(Value::Int).collect()))
        }
        Ty::OutOnly {
            name,
            why: OutOnly::Refinement,
            ..
        } => Err(PyTypeError::new_err(format!(
            "`{name}` is a refined newtype, whose rule is script code the host cannot run, \
             so the host cannot supply one; pass the underlying value and let the script \
             build it"
        ))),
        Ty::OutOnly {
            name,
            why: OutOnly::GenericEnum,
            ..
        } => Err(PyTypeError::new_err(format!(
            "`{name}` is an instance of a const-generic enum, which Keleusma 0.2.2 cannot \
             receive from the host because it records only the generic definition's \
             layout; pass the payload and let the script build the variant"
        ))),
        Ty::Enum { name, variants } => enum_value(obj, ty, name, variants),
        Ty::Option(inner) => {
            if obj.is_none() {
                Ok(Value::None)
            } else {
                let payload = to_value(obj, inner)?;
                Ok(Value::Enum(EnumBody::boxed_with_disc(
                    "Option".to_string(),
                    "Some".to_string(),
                    OPTION_SOME,
                    vec![payload],
                )))
            }
        }
        _ => Err(type_error(ty, obj)),
    }
}

/// A struct from any mapping with exactly the declared fields.
fn struct_value(
    obj: &Bound<'_, PyAny>,
    ty: &Ty,
    name: &str,
    fields: &[(String, Ty)],
) -> PyResult<Value> {
    let mapping = obj.cast::<PyMapping>().map_err(|_| type_error(ty, obj))?;
    if mapping.len()? != fields.len() {
        let declared: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
        return Err(PyTypeError::new_err(format!(
            "expected {name} with the fields {}, got a mapping of {} key(s)",
            declared.join(", "),
            mapping.len()?
        )));
    }
    let values = fields
        .iter()
        .map(|(field, field_ty)| {
            let item = mapping.get_item(field).map_err(|_| {
                PyTypeError::new_err(format!("expected {name} to have the field `{field}`"))
            })?;
            Ok((field.clone(), to_value(&item, field_ty)?))
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok(Value::Struct(StructBody::boxed(name.to_string(), values)))
}

/// An enum value from a `keleusma.Variant`, or from a variant's name for a variant
/// without fields.
fn enum_value(
    obj: &Bound<'_, PyAny>,
    ty: &Ty,
    name: &str,
    variants: &[Variant],
) -> PyResult<Value> {
    let (variant, payload) = variant_of(obj, ty, name, variants)?;
    let values = payload
        .iter()
        .zip(&variant.fields)
        .map(|(item, field_ty)| to_value(item, field_ty))
        .collect::<PyResult<Vec<_>>>()?;
    Ok(Value::Enum(EnumBody::boxed_with_disc(
        name.to_string(),
        variant.name.clone(),
        variant.disc,
        values,
    )))
}

/// The declared variant a `keleusma.Variant` or a variant's name names, and the
/// payload it carries, checked against the variant's field count.
fn variant_of<'a, 'py>(
    obj: &Bound<'py, PyAny>,
    ty: &Ty,
    name: &str,
    variants: &'a [Variant],
) -> PyResult<(&'a Variant, Vec<Bound<'py, PyAny>>)> {
    let variant_class = obj.py().import("keleusma._values")?.getattr("Variant")?;
    let (variant_name, payload): (String, Vec<Bound<'_, PyAny>>) =
        if obj.is_instance(&variant_class)? {
            let values = obj.getattr("values")?;
            let tuple = values
                .cast::<PyTuple>()
                .map_err(|_| PyTypeError::new_err("a Variant's values must be a tuple"))?;
            (obj.getattr("name")?.extract()?, tuple.iter().collect())
        } else if obj.is_instance_of::<PyString>() {
            (obj.extract()?, Vec::new())
        } else {
            return Err(type_error(ty, obj));
        };
    let variant = variants
        .iter()
        .find(|v| v.name == variant_name)
        .ok_or_else(|| {
            let known: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
            PyTypeError::new_err(format!(
                "{name} has no variant `{variant_name}`; its variants are {}",
                known.join(", ")
            ))
        })?;
    if payload.len() != variant.fields.len() {
        return Err(PyTypeError::new_err(format!(
            "{name}::{variant_name} takes {} value(s), got {}",
            variant.fields.len(),
            payload.len()
        )));
    }
    Ok((variant, payload))
}

/// Encode `obj`, of the declared type `ty`, into `buf`, which is exactly the bytes a
/// value of `ty` occupies in a flat layout at `widths`. Used for the shared `data`
/// block, which the binding writes for the host.
///
/// Every byte of `buf` is written, padding included, so a shorter enum variant
/// leaves no stale bytes. Only scalar values reach `write_scalar_le`, which panics on
/// text, opaques and composites. Text and opaque fields are refused, as Keleusma's
/// data-segment rules refuse them.
pub fn write_flat(buf: &mut [u8], obj: &Bound<'_, PyAny>, ty: &Ty, widths: Widths) -> PyResult<()> {
    let layout = ty.layout();
    if buf.len() != widths.size_of(&layout) {
        return Err(VMError::new_err(format!(
            "a slot of {} bytes for {}, which needs {}",
            buf.len(),
            ty.name(),
            widths.size_of(&layout)
        )));
    }
    buf.fill(0);
    let field = |buf: &mut [u8], index: usize, item: &Bound<'_, PyAny>, field_ty: &Ty| {
        let offset = widths
            .offset_of(&layout, index)
            .ok_or_else(|| mismatch(field_ty, "a field outside its composite"))?;
        let size = widths.size_of(&field_ty.layout());
        let slot = offset
            .checked_add(size)
            .and_then(|end| buf.get_mut(offset..end))
            .ok_or_else(|| mismatch(field_ty, "a field past the end of its composite"))?;
        write_flat(slot, item, field_ty, widths)
    };
    match ty {
        Ty::Unit | Ty::Bool | Ty::Byte | Ty::Word | Ty::Fixed(_) | Ty::Float => {
            write_scalar(buf, &to_value(obj, ty)?, widths)
        }
        Ty::Text | Ty::Opaque(_) => Err(PyTypeError::new_err(format!(
            "a data block cannot hold {}, as Keleusma's data-segment rules refuse it",
            ty.name()
        ))),
        Ty::Tuple(types) => {
            let tuple = obj.cast::<PyTuple>().map_err(|_| type_error(ty, obj))?;
            if tuple.len() != types.len() {
                return Err(type_error(ty, obj));
            }
            for (index, (item, field_ty)) in tuple.iter().zip(types).enumerate() {
                field(buf, index, &item, field_ty)?;
            }
            Ok(())
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
                return Err(type_error(ty, obj));
            }
            for (index, item) in items.iter().enumerate() {
                field(buf, index, item, element)?;
            }
            Ok(())
        }
        Ty::Multiword { words, frac_bits } => {
            let list = obj
                .py()
                .import("keleusma._multiword")?
                .getattr("to_words")?
                .call1((obj, *words, *frac_bits))?;
            write_flat(buf, &list, &Ty::Array(Box::new(Ty::Word), *words), widths)
        }
        Ty::Struct { name, fields } => {
            let mapping = obj.cast::<PyMapping>().map_err(|_| type_error(ty, obj))?;
            if mapping.len()? != fields.len() {
                return Err(type_error(ty, obj));
            }
            for (index, (field_name, field_ty)) in fields.iter().enumerate() {
                let item = mapping.get_item(field_name).map_err(|_| {
                    PyTypeError::new_err(format!(
                        "expected {name} to have the field `{field_name}`"
                    ))
                })?;
                field(buf, index, &item, field_ty)?;
            }
            Ok(())
        }
        Ty::Enum { name, variants } => {
            let (variant, payload) = variant_of(obj, ty, name, variants)?;
            write_variant(buf, variant.disc, &payload, &variant.fields, widths)
        }
        Ty::Option(inner) => {
            if obj.is_none() {
                write_variant(buf, OPTION_NONE, &[], &[], widths)
            } else {
                let inner = inner.as_ref().clone();
                write_variant(
                    buf,
                    OPTION_SOME,
                    std::slice::from_ref(obj),
                    &[inner],
                    widths,
                )
            }
        }
        Ty::OutOnly { .. } => to_value(obj, ty).map(|_| ()),
    }
}

/// Write an enum body, the discriminant as a word and then the payload fields in
/// order, the layout the reader's `payload` expects.
fn write_variant(
    buf: &mut [u8],
    disc: i64,
    payload: &[Bound<'_, PyAny>],
    types: &[Ty],
    widths: Widths,
) -> PyResult<()> {
    write_scalar(buf, &Value::Int(disc), widths)?;
    let mut offset = widths.word_bytes;
    for (item, field_ty) in payload.iter().zip(types) {
        let size = widths.size_of(&field_ty.layout());
        let slot = offset
            .checked_add(size)
            .and_then(|end| buf.get_mut(offset..end))
            .ok_or_else(|| mismatch(field_ty, "a payload past the end of its enum"))?;
        write_flat(slot, item, field_ty, widths)?;
        offset += size;
    }
    Ok(())
}

/// Write a scalar value at the start of `buf`. Only the scalar variants reach
/// Keleusma's `write_scalar_le`, which panics on any other.
fn write_scalar(buf: &mut [u8], value: &Value, widths: Widths) -> PyResult<()> {
    match value {
        Value::Unit
        | Value::Bool(_)
        | Value::Byte(_)
        | Value::Int(_)
        | Value::Fixed(_)
        | Value::Float(_) => value
            .write_scalar_le(buf, 0, widths.word_bytes, widths.float_bytes)
            .map_err(|e| VMError::new_err(format!("{e:?}"))),
        other => Err(VMError::new_err(format!(
            "{} is not a scalar a data block can hold",
            kind_name(other)
        ))),
    }
}

/// What reading a value out needs from the VM that produced it.
pub struct ReadContext<'a> {
    pub arena: &'a Arena,
    pub widths: Widths,
    /// Resolves an index into the VM's registry of opaque values, which a flat
    /// composite's opaque field holds. `None` for an index the registry lacks.
    pub opaque: &'a dyn Fn(usize) -> Option<Arc<dyn HostOpaque>>,
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
        (Ty::Opaque(_), Value::Opaque(value)) => crate::opaque::to_py(py, value),
        (Ty::Multiword { words, frac_bits }, value) => {
            let list = to_py(py, value, &multiword_words(*words), ctx)?;
            from_words(py, &list, *frac_bits)
        }
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
        (Ty::Struct { name, fields }, Value::Struct(StructBody::Boxed(boxed))) => {
            let dict = PyDict::new(py);
            for (field, field_ty) in fields {
                let value = boxed
                    .fields
                    .iter()
                    .find(|(n, _)| n == field)
                    .map(|(_, v)| v)
                    .ok_or_else(|| {
                        mismatch(ty, &format!("a {name} without the field `{field}`"))
                    })?;
                dict.set_item(field, to_py(py, value, field_ty, ctx)?)?;
            }
            Ok(dict.into_any())
        }
        (Ty::Enum { variants, .. }, Value::Enum(EnumBody::Boxed(boxed))) => {
            let variant = variants
                .iter()
                .find(|v| v.name == boxed.variant)
                .ok_or_else(|| mismatch(ty, &format!("the variant `{}`", boxed.variant)))?;
            if boxed.fields.len() != variant.fields.len() {
                return Err(mismatch(
                    ty,
                    &format!("`{}` with the wrong payload", boxed.variant),
                ));
            }
            let values = boxed
                .fields
                .iter()
                .zip(&variant.fields)
                .map(|(v, t)| to_py(py, v, t, ctx))
                .collect::<PyResult<Vec<_>>>()?;
            make_variant(py, &variant.name, values)
        }
        (Ty::OutOnly { inner, .. }, value) => to_py(py, value, inner, ctx),
        (Ty::Option(_), Value::None) => Ok(py.None().into_bound(py)),
        (Ty::Option(inner), Value::Enum(EnumBody::Boxed(boxed))) if boxed.type_name == "Option" => {
            match (boxed.variant.as_str(), boxed.fields.as_slice()) {
                ("None", []) => Ok(py.None().into_bound(py)),
                ("Some", [payload]) => to_py(py, payload, inner, ctx),
                _ => Err(mismatch(ty, &format!("an Option `{}`", boxed.variant))),
            }
        }
        (Ty::Tuple(_), Value::Tuple(TupleBody::Flat(body)))
        | (Ty::Array(..), Value::Array(ArrayBody::Flat(body)))
        | (Ty::Struct { .. }, Value::Struct(StructBody::Flat(body)))
        | (Ty::Enum { .. } | Ty::Option(_), Value::Enum(EnumBody::Flat(body))) => {
            flat_body(py, body, ty, ctx)
        }
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

/// Read a value of type `ty` from exactly its flat bytes, outside any composite the
/// VM produced, such as the shared `data` block, which holds no arena references.
pub fn read_flat_value<'py>(
    py: Python<'py>,
    bytes: &[u8],
    ty: &Ty,
    ctx: &ReadContext<'_>,
) -> PyResult<Bound<'py, PyAny>> {
    read_flat(py, bytes, ty, ctx, ctx.arena.epoch())
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
        ..
    } = ctx.widths;
    let layout = ty.layout();
    let expected = ctx.widths.size_of(&layout);
    if bytes.len() != expected {
        return Err(mismatch(
            ty,
            &format!("a body of {} bytes, not {expected}", bytes.len()),
        ));
    }
    match ty {
        Ty::Multiword { words, frac_bits } => {
            let list = read_flat(py, bytes, &multiword_words(*words), ctx, epoch)?;
            from_words(py, &list, *frac_bits)
        }
        Ty::Opaque(name) => {
            let index = Value::read_scalar_le(
                bytes,
                0,
                keleusma::value_layout::ScalarKind::Int,
                word_bytes,
                float_bytes,
            )
            .ok()
            .and_then(|v| match v {
                Value::Int(i) => usize::try_from(i).ok(),
                _ => None,
            });
            let value = index.and_then(|i| (ctx.opaque)(i)).ok_or_else(|| {
                VMError::new_err(format!(
                    "an opaque `{name}` field of a flat composite does not resolve, which a \
                     well-formed value never produces"
                ))
            })?;
            crate::opaque::to_py(py, &value)
        }
        Ty::Text => {
            let text = crate::compat::read_flat_text(bytes, ctx.widths, ctx.arena, epoch)
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
        Ty::OutOnly { inner, .. } => read_flat(py, bytes, inner, ctx, epoch),
        Ty::Struct {
            fields: declared, ..
        } => {
            let items = fields(
                py,
                bytes,
                &layout,
                declared.iter().map(|(_, t)| t),
                ctx,
                epoch,
            )?;
            let dict = PyDict::new(py);
            for ((field, _), item) in declared.iter().zip(items) {
                dict.set_item(field, item)?;
            }
            Ok(dict.into_any())
        }
        Ty::Enum { variants, .. } => {
            let disc = read_disc(bytes, word_bytes, float_bytes, ty)?;
            let variant = variants
                .iter()
                .find(|v| v.disc == disc)
                .ok_or_else(|| mismatch(ty, &format!("the discriminant {disc}")))?;
            let values = payload(py, bytes, &variant.fields, ctx, epoch)?;
            make_variant(py, &variant.name, values)
        }
        Ty::Option(inner) => {
            let disc = read_disc(bytes, word_bytes, float_bytes, ty)?;
            match disc {
                OPTION_NONE => Ok(py.None().into_bound(py)),
                OPTION_SOME => {
                    let mut values =
                        payload(py, bytes, std::slice::from_ref(inner.as_ref()), ctx, epoch)?;
                    values
                        .pop()
                        .ok_or_else(|| mismatch(ty, "a Some without a payload"))
                }
                other => Err(mismatch(ty, &format!("the Option discriminant {other}"))),
            }
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

/// The discriminant in a flat enum body's leading word.
fn read_disc(bytes: &[u8], word_bytes: usize, float_bytes: usize, ty: &Ty) -> PyResult<i64> {
    match Value::read_scalar_le(
        bytes,
        0,
        keleusma::value_layout::ScalarKind::Int,
        word_bytes,
        float_bytes,
    ) {
        Ok(Value::Int(disc)) => Ok(disc),
        _ => Err(mismatch(ty, "an enum body without a discriminant")),
    }
}

/// Read a flat enum variant's payload, packed after the discriminant word.
fn payload<'py>(
    py: Python<'py>,
    bytes: &[u8],
    types: &[Ty],
    ctx: &ReadContext<'_>,
    epoch: u64,
) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let mut offset = ctx.widths.word_bytes;
    types
        .iter()
        .map(|field| {
            let size = ctx.widths.size_of(&field.layout());
            let slice = offset
                .checked_add(size)
                .and_then(|end| bytes.get(offset..end))
                .ok_or_else(|| mismatch(field, "a payload past the end of its enum"))?;
            offset += size;
            read_flat(py, slice, field, ctx, epoch)
        })
        .collect()
}

/// The `[Word; N]` a multiword is laid out as, which its words are read as.
fn multiword_words(words: usize) -> Ty {
    Ty::Array(Box::new(Ty::Word), words)
}

/// The Python number a multiword's words represent, by `keleusma._multiword`.
fn from_words<'py>(
    py: Python<'py>,
    words: &Bound<'py, PyAny>,
    frac_bits: u32,
) -> PyResult<Bound<'py, PyAny>> {
    py.import("keleusma._multiword")?
        .getattr("from_words")?
        .call1((words, frac_bits))
}

fn make_variant<'py>(
    py: Python<'py>,
    name: &str,
    values: Vec<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    py.import("keleusma._values")?
        .getattr("Variant")?
        .call1((name, PyTuple::new(py, values)?))
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
    types
        .enumerate()
        .map(|(index, field)| {
            let offset = ctx
                .widths
                .offset_of(layout, index)
                .ok_or_else(|| mismatch(field, "a field outside its composite"))?;
            let size = ctx.widths.size_of(&field.layout());
            let slice = offset
                .checked_add(size)
                .and_then(|end| bytes.get(offset..end))
                .ok_or_else(|| mismatch(field, "a field past the end of its composite"))?;
            read_flat(py, slice, field, ctx, epoch)
        })
        .collect()
}

/// Convert a value with no declared type, as a bare `use host::name` gives a native's
/// arguments, and as every value of a module loaded from bytes is. Scalars carry their own tags and convert. A `Fixed` has no fraction-bit
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
        // An opaque value carries its own type name, so it needs no declaration.
        Value::Opaque(value) => crate::opaque::to_py(py, value),
        other => Err(PyTypeError::new_err(format!(
            "{} needs a declared type to convert, which {UNTYPED_HINT}",
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
        "a value of type {got} needs a declared type to convert, which {UNTYPED_HINT}"
    )))
}

/// Where a declared type would have come from, for the untyped conversion errors.
const UNTYPED_HINT: &str = "a native gains from a signature such as \
    `use host::name(T1, T2) -> R`, and which a module loaded from bytes does not have, \
    since only scalars convert without one";

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
