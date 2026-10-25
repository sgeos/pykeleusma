//! Roadmap task 3. `keleusma.VM(module)`, a verified VM with its own arena.
//!
//! The ownership and the arena sizing live in [`crate::owned_vm`]. This module is the
//! Python face of it, and maps construction failures onto Python exceptions.
//!
//! | Failure | Python exception |
//! |---|---|
//! | The verifier rejects the module | `keleusma.VerifyError`, with the verifier's message |
//! | Any other Keleusma error | `keleusma.VMError`, with Keleusma's description |
//! | The arena cannot be allocated | `MemoryError` |
//! | The capacity overflows `usize` | `OverflowError` |
//! | The persistent region does not fit | `RuntimeError`, which would be a binding defect |

use keleusma::bytecode::Value;
use keleusma::vm::{VmError, VmState};
use pyo3::exceptions::{PyMemoryError, PyOverflowError, PyRuntimeError, PyTypeError};
use pyo3::prelude::*;

use crate::compile::Module;
use crate::owned_vm::{OwnedVm, OwnedVmError};

pyo3::import_exception!(keleusma._errors, VerifyError);
pyo3::import_exception!(keleusma._errors, VMError);

/// The Python exception for a Keleusma VM error.
///
/// A verification failure passes the verifier's message through unchanged. Every
/// other error is described by its `Debug` form, because `VmError` implements `Debug`
/// but not `Display`.
pub fn vm_error(error: &VmError) -> PyErr {
    match error {
        VmError::VerifyError(message) => VerifyError::new_err(message.clone()),
        other => VMError::new_err(format!("{other:?}")),
    }
}

fn construction_error(error: OwnedVmError) -> PyErr {
    match error {
        OwnedVmError::Vm(inner) => vm_error(&inner),
        OwnedVmError::ArenaAllocation { .. } => PyMemoryError::new_err(error.to_string()),
        OwnedVmError::CapacityOverflow => PyOverflowError::new_err(error.to_string()),
        OwnedVmError::PersistentRegion { .. } => PyRuntimeError::new_err(error.to_string()),
    }
}

/// Convert the scalar results the temporary test hook needs. Task 4 replaces this
/// with full conversion, so anything else is refused rather than guessed at.
fn scalar_result(value: &Value) -> PyResult<Option<i64>> {
    match value {
        Value::Unit => Ok(None),
        Value::Int(n) => Ok(Some(*n)),
        other => Err(PyTypeError::new_err(format!(
            "value conversion is not implemented yet for {other:?}"
        ))),
    }
}

/// A verified Keleusma VM that owns its arena.
///
/// `unsendable`, because the VM is neither `Send` nor `Sync`. A VM belongs to the
/// thread that created it. PyO3 refuses use from any other thread, and drops the
/// object on the creating thread.
#[pyclass(unsendable, module = "keleusma._keleusma", name = "VM")]
pub struct VM {
    vm: OwnedVm,
}

#[pymethods]
impl VM {
    #[new]
    #[pyo3(signature = (module, *, argument_bytes = crate::DEFAULT_ARGUMENT_BYTES))]
    fn new(module: PyRef<'_, Module>, argument_bytes: usize) -> PyResult<Self> {
        // `Vm::new` consumes the module, so the VM takes a copy and the Python module
        // stays usable for further VMs.
        let vm =
            OwnedVm::new(module.inner().clone(), argument_bytes).map_err(construction_error)?;
        Ok(Self { vm })
    }

    /// The arena's total capacity in bytes.
    #[getter]
    fn arena_capacity(&self) -> usize {
        self.vm.arena_capacity()
    }

    /// The bytes of the arena reserved for composite arguments.
    #[getter]
    fn argument_bytes(&self) -> usize {
        self.vm.plan().argument_bytes
    }

    fn __repr__(&self) -> String {
        format!(
            "<keleusma.VM with an arena of {} bytes>",
            self.vm.arena_capacity()
        )
    }

    /// Call the entry point with no arguments, returning the state name and an integer
    /// or `None`. A temporary hook for tests, removed when task 5 adds `call`.
    fn _call_no_args(&mut self) -> PyResult<(&'static str, Option<i64>)> {
        let state = self.vm.call(&[]).map_err(|e| vm_error(&e))?;
        match state {
            VmState::Yielded(value) => Ok(("yielded", scalar_result(&value)?)),
            VmState::Finished(value) => Ok(("finished", scalar_result(&value)?)),
            VmState::Reset => Ok(("reset", None)),
            VmState::BreakpointHit { .. } => Err(PyRuntimeError::new_err(
                "a breakpoint was hit, but the binding arms none",
            )),
        }
    }
}
