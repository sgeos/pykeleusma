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

use keleusma::vm::{VmError, VmState};
use pyo3::exceptions::{PyMemoryError, PyOverflowError, PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::compile::{Boundary, Module};
use crate::convert::{ReadContext, to_py, to_value};
use crate::owned_vm::{OwnedVm, OwnedVmError};
use crate::types::{Declared, Ty};

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

/// The declared type, or the `TypeError` that explains why it cannot be converted.
fn declared<'a>(declared: &'a Declared, role: &str) -> PyResult<&'a Ty> {
    declared
        .as_ref()
        .map_err(|unsupported| PyTypeError::new_err(format!("{role} has {}", unsupported.0)))
}

/// A verified Keleusma VM that owns its arena.
///
/// `unsendable`, because the VM is neither `Send` nor `Sync`. A VM belongs to the
/// thread that created it. PyO3 refuses use from any other thread, and drops the
/// object on the creating thread.
#[pyclass(unsendable, module = "keleusma._keleusma", name = "VM")]
pub struct VM {
    vm: OwnedVm,
    boundary: Boundary,
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
        Ok(Self {
            vm,
            boundary: module.boundary().clone(),
        })
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

    /// Call the entry point, converting the arguments and the result by the declared
    /// types. Returns the state name and the converted value, or `None` for a reset.
    /// A temporary hook for tests, replaced by `call` in roadmap task 5.
    #[pyo3(signature = (*args))]
    fn _call<'py>(
        &mut self,
        py: Python<'py>,
        args: &Bound<'py, PyTuple>,
    ) -> PyResult<(&'static str, Bound<'py, PyAny>)> {
        let signature = self
            .boundary
            .signature
            .as_ref()
            .ok_or_else(|| PyTypeError::new_err("the module has no `main`"))?;
        if args.len() != signature.params.len() {
            return Err(PyTypeError::new_err(format!(
                "main takes {} argument(s), got {}",
                signature.params.len(),
                args.len()
            )));
        }
        let values = args
            .iter()
            .zip(&signature.params)
            .enumerate()
            .map(|(index, (arg, param))| {
                let ty = declared(param, &format!("argument {}", index + 1))?;
                to_value(&arg, ty)
            })
            .collect::<PyResult<Vec<_>>>()?;
        let state = self.vm.call(&values).map_err(|e| vm_error(&e))?;
        // Convert before anything else runs on this VM, while a flat result is valid.
        let ctx = ReadContext {
            arena: self.vm.arena(),
            widths: self.boundary.widths,
        };
        let (name, value) = match state {
            VmState::Yielded(value) => ("yielded", value),
            VmState::Finished(value) => ("finished", value),
            VmState::Reset => return Ok(("reset", py.None().into_bound(py))),
            VmState::BreakpointHit { .. } => {
                return Err(PyRuntimeError::new_err(
                    "a breakpoint was hit, but the binding arms none",
                ));
            }
        };
        let ty = declared(&signature.ret, "the result")?;
        Ok((name, to_py(py, &value, ty, &ctx)?))
    }
}
