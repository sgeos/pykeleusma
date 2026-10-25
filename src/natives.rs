//! Roadmap task 7. Python callables as Keleusma natives.
//!
//! # Shape
//!
//! At construction the binding registers one Keleusma closure for every native the
//! module names. The closure does not hold the Python callable. It looks the callable
//! up by name in a [`Registry`] shared with the VM object. Three things follow.
//!
//! - **Registration is an update.** `vm.register` replaces a registry entry rather than
//!   registering a new Keleusma closure, so the arena sizing and verification done at
//!   construction stay valid.
//! - **The garbage collector can see the callables.** The VM's `__traverse__` visits
//!   the registry, so a native bound to an object that owns the VM forms a cycle Python
//!   can collect. A callable captured inside a Keleusma closure would be invisible.
//! - **A missing callable is detected before the VM runs.** The VM refuses to call or
//!   resume while any entry is empty, so the placeholder path in the closure is a
//!   defence, not the expected route.
//!
//! # Exceptions
//!
//! A Keleusma native returns `Result<Value, VmError>`, which cannot carry a Python
//! exception. When the callable raises, the closure stores the exception in an
//! [`ErrorSlot`] and returns an error. For a `keleusma.NativeError` that error is
//! `VmError::NativeErrorCode`, which the script may handle with its native-error
//! construct, the book's chapter 34. For any other exception it is a halting error,
//! which no script construct catches. If the error ends the run, the VM raises the
//! stored exception itself, so the caller of `call` or `resume` sees exactly what the
//! native raised, including `KeyboardInterrupt`. The VM is marked failed, because
//! Keleusma leaves its state undefined after a runtime error.
//!
//! # Re-entry
//!
//! A native that calls back into the same VM, for example `vm.call` from inside a
//! native, is refused by PyO3's borrow checking with an exception, because the VM is
//! already mutably borrowed for the running call. The registry's `RefCell` is borrowed
//! only to clone the callable and is released before Python runs, so a native that
//! touches the registry indirectly cannot cause a borrow panic.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use keleusma::bytecode::Value;
use keleusma::vm::{NativeCtx, VmError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::convert::{ReadContext, Widths, to_py, to_py_untyped, to_value, to_value_untyped};
use crate::types::{Declared, NativeDecl, Ty};

/// One native's callable, if registered, and its declared bounds.
pub struct Slot {
    pub callable: Option<Py<PyAny>>,
    pub wcet: u32,
    pub wcmu_bytes: u32,
    /// Whether one of Keleusma's bundled libraries provides this native, in which
    /// case Keleusma calls its own implementation and `callable` is unused.
    pub library: bool,
    /// For a `use external` native, the host's attested bound on calls per stream
    /// iteration, and `None` for any other native.
    pub max_invocations: Option<u32>,
}

/// The natives of one VM by full name, shared with the closures Keleusma holds.
pub type Registry = Rc<RefCell<BTreeMap<String, Slot>>>;

/// The exception a native raised during the current call, for the VM to re-raise.
pub type ErrorSlot = Rc<RefCell<Option<PyErr>>>;

fn declared<'a>(declared: &'a Declared, role: &str) -> PyResult<&'a Ty> {
    declared.as_ref().map_err(|unsupported| {
        pyo3::exceptions::PyTypeError::new_err(format!("{role} has {}", unsupported.0))
    })
}

/// Build the Keleusma closure for the native `name`.
pub fn closure(
    name: String,
    decl: Option<NativeDecl>,
    registry: Registry,
    errors: ErrorSlot,
) -> impl for<'b> Fn(&NativeCtx<'b>, &[Value]) -> Result<Value, VmError> + 'static {
    move |ctx: &NativeCtx<'_>, args: &[Value]| {
        Python::attach(
            |py| match invoke(py, &name, decl.as_ref(), &registry, ctx, args) {
                Ok(value) => Ok(value),
                Err(error) => {
                    // A `keleusma.NativeError` reports a code the script may handle with
                    // its native-error construct. Anything else fails the call, so it is
                    // reported as a halting error, which that construct does not catch.
                    // Keleusma's own `NativeError` would be caught, with the code -1, and
                    // would swallow even `KeyboardInterrupt`.
                    let code = native_error_code(py, &error);
                    let kind = error
                        .get_type(py)
                        .name()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|_| "an exception".to_string());
                    *errors.borrow_mut() = Some(error);
                    Err(match code {
                        Some((code, message)) => VmError::NativeErrorCode { code, message },
                        None => VmError::InvalidBytecode(format!(
                            "{NATIVE_RAISED} `{name}` raised {kind}"
                        )),
                    })
                }
            },
        )
    }
}

/// The start of the message of the halting error a native returns when its callable
/// raised an exception other than `keleusma.NativeError`. Keleusma has no halting
/// error for a host failure, so `InvalidBytecode` carries it. KELEUSMA-0.2.2: the
/// 0.2.3 branch adds no such variant either. The VM never shows the
/// message, because it raises the stored exception instead.
pub const NATIVE_RAISED: &str = "pykeleusma: the native";

/// Whether `error` is one a native closure returned after storing an exception, so
/// the stored exception, not `error`, is what the caller sees. Any other error means a
/// stored exception, if any, was handled by the script and is stale.
pub fn carries_stored_exception(error: &VmError) -> bool {
    match error {
        VmError::NativeErrorCode { .. } => true,
        VmError::InvalidBytecode(message) => message.starts_with(NATIVE_RAISED),
        _ => false,
    }
}

/// The code and message of a `keleusma.NativeError`, or `None` for any other
/// exception, including a `NativeError` whose code does not fit a `Word`.
fn native_error_code(py: Python<'_>, error: &PyErr) -> Option<(i64, String)> {
    let class = py
        .import("keleusma._errors")
        .and_then(|m| m.getattr("NativeError"))
        .ok()?;
    let value = error.value(py);
    if !value.is_instance(&class).ok()? {
        return None;
    }
    let code = value.getattr("code").ok()?.extract::<i64>().ok()?;
    let message = value
        .getattr("message")
        .ok()
        .and_then(|m| m.extract::<String>().ok())
        .unwrap_or_default();
    Some((code, message))
}

fn invoke(
    py: Python<'_>,
    name: &str,
    decl: Option<&NativeDecl>,
    registry: &Registry,
    ctx: &NativeCtx<'_>,
    args: &[Value],
) -> PyResult<Value> {
    // Clone the callable and release the borrow before any Python code runs.
    let callable = registry
        .borrow()
        .get(name)
        .and_then(|slot| slot.callable.as_ref().map(|c| c.clone_ref(py)))
        .ok_or_else(|| crate::vm::VMError::new_err(format!("native `{name}` is not registered")))?;
    let resolve = |i: usize| ctx.opaques.get(i).cloned();
    let read = ReadContext {
        arena: ctx.arena,
        widths: Widths::of_native(ctx),
        opaque: &resolve,
    };
    let signature = decl.and_then(|d| d.signature.as_ref());
    let converted = match signature {
        Some((params, _)) => {
            if params.len() != args.len() {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "native `{name}` declares {} parameter(s) but received {}",
                    params.len(),
                    args.len()
                )));
            }
            args.iter()
                .zip(params)
                .enumerate()
                .map(|(index, (arg, param))| {
                    let ty = declared(param, &format!("parameter {} of `{name}`", index + 1))?;
                    to_py(py, arg, ty, &read)
                })
                .collect::<PyResult<Vec<_>>>()?
        }
        None => args
            .iter()
            .map(|arg| to_py_untyped(py, arg, &read))
            .collect::<PyResult<Vec<_>>>()?,
    };
    let result = callable.bind(py).call1(PyTuple::new(py, converted)?)?;
    match signature {
        Some((_, ret)) => to_value(&result, declared(ret, &format!("the result of `{name}`"))?),
        None => to_value_untyped(&result),
    }
}
