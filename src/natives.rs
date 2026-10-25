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
//! [`ErrorSlot`] and returns `VmError::NativeError`. The VM then raises the stored
//! exception itself, so the caller of `call` or `resume` sees exactly what the native
//! raised, including `KeyboardInterrupt`. The VM is marked failed, because Keleusma
//! leaves its state undefined after a runtime error.
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
                    let kind = error
                        .get_type(py)
                        .name()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|_| "an exception".to_string());
                    *errors.borrow_mut() = Some(error);
                    Err(VmError::NativeError(format!(
                        "native `{name}` raised {kind}"
                    )))
                }
            },
        )
    }
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
    let read = ReadContext {
        arena: ctx.arena,
        widths: Widths {
            word_bytes: ctx.word_bytes,
            float_bytes: ctx.float_bytes,
        },
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
