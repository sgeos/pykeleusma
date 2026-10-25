//! Opaque host values, which the Keleusma book's embedding guide describes.
//!
//! A script may name a type it does not define, such as `RustString`, and Keleusma's
//! type checker treats that name as an opaque host type. The script can hold, pass
//! and return such a value but cannot look inside it. In this binding any Python
//! object can be passed where an opaque type is declared, and the same object comes
//! back out, so a script can carry handles to host state, such as a sprite or a file,
//! without the binding converting them.
//!
//! # Identity and lifetime
//!
//! Keleusma compares opaque values by the identity of their `Arc`. Each time a Python
//! object crosses into the VM it is wrapped in a new `Arc`, so a script sees two
//! crossings of one object as two values. The wrapper holds a strong reference to the Python
//! object, which lives as long as the script or the VM's registry holds the value.
//!
//! # Trust
//!
//! A Python object is not bounded memory, so an opaque value is outside Keleusma's
//! guarantees in the same way as a Python native. Keleusma counts an opaque value as
//! zero arena bytes because the host owns it.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use keleusma::bytecode::Value;
use keleusma::opaque::HostOpaque;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;

/// A Python object carried as an opaque value under the script's type name.
struct PyOpaque {
    name: &'static str,
    object: Py<PyAny>,
}

impl HostOpaque for PyOpaque {
    fn type_name(&self) -> &'static str {
        self.name
    }
}

/// Keleusma requires an opaque type's name as a `&'static str`. Each distinct name is
/// leaked once and reused, so the memory spent is bounded by the number of distinct
/// opaque type names the process's scripts declare, not by the number of values.
fn intern(name: &str) -> &'static str {
    static NAMES: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    // A poisoned lock still holds a valid set, because insertion cannot leave it
    // half-updated, so recovery is sound.
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = names.get(name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.insert(leaked);
    leaked
}

/// The opaque value for `object` under the declared type `name`, a new wrapper that
/// holds a strong reference to it.
pub fn to_value(object: &Bound<'_, PyAny>, name: &str) -> Value {
    Value::Opaque(Arc::new(PyOpaque {
        name: intern(name),
        object: object.clone().unbind(),
    }))
}

/// The Python object an opaque value wraps.
///
/// Every opaque value in this binding starts as a Python object, because the bundled
/// libraries produce none, so another kind means a value this binding did not create
/// and is refused rather than exposed.
pub fn to_py<'py>(py: Python<'py>, value: &Arc<dyn HostOpaque>) -> PyResult<Bound<'py, PyAny>> {
    value
        .as_ref()
        .downcast_ref::<PyOpaque>()
        .map(|wrapped| wrapped.object.bind(py).clone())
        .ok_or_else(|| {
            PyTypeError::new_err(format!(
                "an opaque `{}` that did not come from Python cannot be converted",
                value.type_name()
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_interned_once() {
        let first = intern("Sprite");
        let second = intern(&String::from("Sprite"));
        assert!(std::ptr::eq(first, second));
        assert_ne!(intern("Other"), first);
    }
}
