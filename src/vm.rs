//! Roadmap tasks 3, 5 and 6. `keleusma.VM(module)`, a verified VM with its own arena,
//! its `call` and `resume`, and `stream`.
//!
//! The ownership and the arena sizing live in [`crate::owned_vm`]. This module is the
//! Python face of it, maps failures onto Python exceptions, and converts values by the
//! declared types through [`crate::convert`].
//!
//! # After a runtime error
//!
//! Keleusma's execution model states that a runtime error during `call` or `resume`
//! leaves the VM "in an undefined intermediate state". The binding therefore marks the
//! VM unusable after any such error, and every later `call` or `resume` raises
//! `keleusma.VMError`. Two kinds of failure leave it usable, because nothing ran. A
//! conversion error is raised before the VM is entered, and `NotSuspended`, a resume
//! with nothing to resume, is returned by Keleusma before it changes anything.
//!
//! | Failure | Python exception |
//! |---|---|
//! | The verifier rejects the module | `keleusma.VerifyError`, with the verifier's message |
//! | Any other Keleusma error | `keleusma.VMError`, with Keleusma's description |
//! | The arena cannot be allocated | `MemoryError` |
//! | The capacity overflows `usize` | `OverflowError` |
//! | The persistent region does not fit | `RuntimeError`, which would be a binding defect |

use keleusma::bytecode::BlockType;
use keleusma::vm::{VmError, VmState};
use pyo3::exceptions::{PyMemoryError, PyOverflowError, PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use crate::compile::{Boundary, Module};
use crate::convert::{ReadContext, to_py, to_value};
use crate::owned_vm::{OwnedVm, OwnedVmError};
use crate::types::{Declared, EntrySignature, Ty};

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
    /// Set by a runtime error, after which the VM's state is undefined.
    failed: bool,
    /// Whether the module has a stream chunk, the only kind with a per-iteration bound.
    has_stream: bool,
}

#[pymethods]
impl VM {
    #[new]
    #[pyo3(signature = (module, *, argument_bytes = crate::DEFAULT_ARGUMENT_BYTES))]
    fn new(module: PyRef<'_, Module>, argument_bytes: usize) -> PyResult<Self> {
        // `Vm::new` consumes the module, so the VM takes a copy and the Python module
        // stays usable for further VMs.
        let has_stream = module
            .inner()
            .chunks
            .iter()
            .any(|chunk| chunk.block_type == BlockType::Stream);
        let vm =
            OwnedVm::new(module.inner().clone(), argument_bytes).map_err(construction_error)?;
        Ok(Self {
            vm,
            boundary: module.boundary().clone(),
            failed: false,
            has_stream,
        })
    }

    /// The arena's total capacity in bytes, which bounds the memory the script itself
    /// can use. See "Worst-case figures" in the design document for what it covers.
    #[getter]
    fn arena_capacity(&self) -> usize {
        self.vm.arena_capacity()
    }

    /// The worst-case execution time of one iteration of a `loop main`, in Keleusma's
    /// nominal cost units, or `None` for a module without a stream.
    ///
    /// The units are unitless, on the scale of Keleusma's per-instruction costs, not
    /// cycles or seconds. The figure covers the script and the declared bounds of any
    /// natives, never Python or the binding's own conversions.
    #[getter]
    fn wcet_per_iteration(&self) -> PyResult<Option<u32>> {
        // Keleusma reports zero for a module with no stream chunk, which would read as
        // a free iteration rather than an absent figure.
        if !self.has_stream {
            return Ok(None);
        }
        self.vm
            .wcet_per_iteration()
            .map(Some)
            .map_err(|e| vm_error(&e))
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

    /// Call the entry point with `args`, converted by the declared parameter types.
    ///
    /// Returns `Yielded`, `Finished` or `Reset`, with the value converted by the
    /// declared return type.
    #[pyo3(signature = (*args))]
    fn call<'py>(
        &mut self,
        py: Python<'py>,
        args: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.check_usable()?;
        let signature = self.signature()?;
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
        let outcome = self.vm.call(&values);
        self.state(py, outcome)
    }

    /// Resume after a yield or a reset with `value`, converted by the type of the
    /// stream's parameter.
    ///
    /// Returns `Yielded`, `Finished` or `Reset`. Raises `keleusma.VMError` if there is
    /// nothing to resume, for example before `call` or after `Finished`.
    fn resume<'py>(
        &mut self,
        py: Python<'py>,
        value: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.check_usable()?;
        let ty = declared(&self.signature()?.resume, "resume")?;
        let input = to_value(value, ty)?;
        let outcome = self.vm.resume(input);
        self.state(py, outcome)
    }

    /// Drive the VM through Python's generator protocol, yielding each `Yielded` and
    /// `Reset` and returning the `Finished` value. Resume inputs come from `inputs`
    /// or from `send`. The semantics, including the meaning of `Reset`, are
    /// documented in `keleusma._stream`, which implements the generator, because a
    /// generator is far simpler to write and to read in Python than through PyO3.
    #[pyo3(signature = (*args, inputs = None))]
    fn stream<'py>(
        slf: Bound<'py, Self>,
        args: &Bound<'py, PyTuple>,
        inputs: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let mut positional = vec![slf.into_any()];
        positional.extend(args.iter());
        let keywords = PyDict::new(py);
        keywords.set_item("inputs", inputs)?;
        py.import("keleusma._stream")?
            .getattr("stream")?
            .call(PyTuple::new(py, positional)?, Some(&keywords))
    }

    /// Whether an earlier runtime error has made this VM unusable.
    #[getter]
    fn failed(&self) -> bool {
        self.failed
    }
}

impl VM {
    fn signature(&self) -> PyResult<&EntrySignature> {
        self.boundary
            .signature
            .as_ref()
            .ok_or_else(|| PyTypeError::new_err("the module has no `main`"))
    }

    fn check_usable(&self) -> PyResult<()> {
        if self.failed {
            return Err(VMError::new_err(
                "this VM failed earlier and its state is undefined; build a new VM",
            ));
        }
        Ok(())
    }

    /// Turn the outcome of a call or resume into a Python state object, converting
    /// the value before anything else runs on this VM, while a flat result is valid.
    fn state<'py>(
        &mut self,
        py: Python<'py>,
        outcome: Result<VmState, VmError>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let state = match outcome {
            Ok(state) => state,
            Err(VmError::NotSuspended) => return Err(vm_error(&VmError::NotSuspended)),
            Err(error) => {
                self.failed = true;
                return Err(vm_error(&error));
            }
        };
        let states = py.import("keleusma._state")?;
        let (class, value) = match state {
            VmState::Yielded(value) => ("Yielded", value),
            VmState::Finished(value) => ("Finished", value),
            VmState::Reset => return states.getattr("Reset")?.call0(),
            VmState::BreakpointHit { .. } => {
                self.failed = true;
                return Err(PyRuntimeError::new_err(
                    "a breakpoint was hit, but the binding arms none",
                ));
            }
        };
        let ty = declared(&self.signature()?.ret, "the result")?;
        let ctx = ReadContext {
            arena: self.vm.arena(),
            widths: self.boundary.widths,
        };
        let converted = to_py(py, &value, ty, &ctx)?;
        states.getattr(class)?.call1((converted,))
    }
}
