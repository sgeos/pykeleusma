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

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use keleusma::bytecode::BlockType;
use keleusma::vm::{DEFAULT_NATIVE_WCET, DEFAULT_NATIVE_WCMU_BYTES, VmError, VmState};
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{
    PyKeyError, PyMemoryError, PyOverflowError, PyRuntimeError, PyTypeError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use pyo3::{PyTraverseError, PyVisit};

use crate::compile::{Boundary, Module};
use crate::convert::{ReadContext, to_py, to_value};
use crate::natives::{ErrorSlot, Registry, Slot, closure};
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

/// A host buffer lent to one call as the shared `data` block.
///
/// The buffer is copied into a Rust vector before the call and copied back after it
/// succeeds, rather than lent in place. Lending the Python memory itself would let a
/// Python native write to it while Keleusma holds a mutable borrow of the same bytes,
/// which Rust's aliasing rules forbid, and it would need `unsafe` code. The cost is
/// one copy each way, of `shared_data_bytes`, which is small by construction. A
/// native running during the call sees the buffer as it was before the call. After a
/// failed call the buffer is left as it was, because the VM's state is then undefined.
struct Lent {
    buffer: Option<PyBuffer<u8>>,
    bytes: Vec<u8>,
}

impl Lent {
    fn none() -> Self {
        Self {
            buffer: None,
            bytes: Vec::new(),
        }
    }

    fn new(object: &Bound<'_, PyAny>, size: usize) -> PyResult<Self> {
        let buffer = PyBuffer::<u8>::get(object).map_err(|_| {
            PyTypeError::new_err(
                "`shared` must support the buffer protocol with byte items, such as a \
                 bytearray, a memoryview of one, or a NumPy uint8 array",
            )
        })?;
        if buffer.readonly() {
            return Err(PyTypeError::new_err(
                "`shared` must be writable, because the script may change it",
            ));
        }
        if !buffer.is_c_contiguous() {
            return Err(PyValueError::new_err("`shared` must be contiguous"));
        }
        if buffer.item_count() != size {
            return Err(PyValueError::new_err(format!(
                "`shared` has {} bytes, but the module's shared `data` block has {size}",
                buffer.item_count()
            )));
        }
        let bytes = buffer.to_vec(object.py())?;
        Ok(Self {
            buffer: Some(buffer),
            bytes,
        })
    }

    fn bytes(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    /// Copy the script's changes back into the host buffer.
    fn give_back(self, py: Python<'_>) -> PyResult<()> {
        match self.buffer {
            Some(buffer) => buffer.copy_from_slice(py, &self.bytes),
            None => Ok(()),
        }
    }
}

fn unknown_native(name: &str, names: &[String]) -> PyErr {
    let known = if names.is_empty() {
        "it uses no natives".to_string()
    } else {
        format!("it uses {}", names.join(", "))
    };
    PyKeyError::new_err(format!(
        "the module does not use a native named `{name}`; {known}"
    ))
}

/// The callable and declared bounds of a `natives=` value, which is either a callable
/// or a `keleusma.Native`.
fn native_parts(value: &Bound<'_, PyAny>) -> PyResult<(Py<PyAny>, Option<u32>, Option<u32>)> {
    let native_class = value.py().import("keleusma._native")?.getattr("Native")?;
    if value.is_instance(&native_class)? {
        let function = value.getattr("function")?;
        if !function.is_callable() {
            return Err(PyTypeError::new_err("a Native's function must be callable"));
        }
        let wcet: Option<u32> = value.getattr("wcet")?.extract()?;
        let wcmu_bytes: Option<u32> = value.getattr("wcmu_bytes")?.extract()?;
        return Ok((function.unbind(), wcet, wcmu_bytes));
    }
    if value.is_callable() {
        return Ok((value.clone().unbind(), None, None));
    }
    Err(PyTypeError::new_err(
        "a native must be a callable or a keleusma.Native",
    ))
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
    /// The natives by name, shared with the closures Keleusma holds.
    registry: Registry,
    /// An exception raised by a native during the current call, to re-raise.
    native_error: ErrorSlot,
}

#[pymethods]
impl VM {
    #[new]
    #[pyo3(signature = (module, *, argument_bytes = crate::DEFAULT_ARGUMENT_BYTES, natives = None))]
    fn new(
        module: PyRef<'_, Module>,
        argument_bytes: usize,
        natives: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let names = module.inner().native_names.clone();
        let boundary = module.boundary().clone();
        if let Some(external) = names
            .iter()
            .find(|name| boundary.natives.get(*name).is_some_and(|d| d.external))
        {
            return Err(VerifyError::new_err(format!(
                "the native `{external}` is declared `use external`, which pykeleusma 0.1.0 \
                 does not support"
            )));
        }
        // Every native the module names gets a slot, with Keleusma's default bounds
        // unless the host declares its own, so the arena is sized for all of them.
        let mut slots: BTreeMap<String, Slot> = names
            .iter()
            .map(|name| {
                let slot = Slot {
                    callable: None,
                    wcet: DEFAULT_NATIVE_WCET,
                    wcmu_bytes: DEFAULT_NATIVE_WCMU_BYTES,
                };
                (name.clone(), slot)
            })
            .collect();
        if let Some(natives) = natives {
            let items = natives.call_method0("items")?;
            for item in items.try_iter()? {
                let (name, value): (String, Bound<'_, PyAny>) = item?.extract()?;
                let slot = slots
                    .get_mut(&name)
                    .ok_or_else(|| unknown_native(&name, &names))?;
                let (callable, wcet, wcmu_bytes) = native_parts(&value)?;
                slot.callable = Some(callable);
                slot.wcet = wcet.unwrap_or(DEFAULT_NATIVE_WCET);
                slot.wcmu_bytes = wcmu_bytes.unwrap_or(DEFAULT_NATIVE_WCMU_BYTES);
            }
        }
        let native_wcmu: Vec<u32> = names.iter().map(|n| slots[n].wcmu_bytes).collect();
        let bounds: Vec<(String, u32, u32)> = names
            .iter()
            .map(|n| (n.clone(), slots[n].wcet, slots[n].wcmu_bytes))
            .collect();
        let has_stream = module
            .inner()
            .chunks
            .iter()
            .any(|chunk| chunk.block_type == BlockType::Stream);
        // `Vm::new` consumes the module, so the VM takes a copy and the Python module
        // stays usable for further VMs.
        let mut vm = OwnedVm::new(module.inner().clone(), argument_bytes, &native_wcmu)
            .map_err(construction_error)?;
        let registry: Registry = Rc::new(RefCell::new(slots));
        let native_error: ErrorSlot = Rc::new(RefCell::new(None));
        for (name, wcet, wcmu_bytes) in bounds {
            let decl = boundary.natives.get(&name).cloned();
            let func = closure(name.clone(), decl, registry.clone(), native_error.clone());
            vm.register_native(&name, func);
            vm.set_native_bounds(&name, wcet, wcmu_bytes)
                .map_err(|e| vm_error(&e))?;
        }
        // The arena was sized with these bounds, so this confirms the sizing rather
        // than expecting to fail.
        vm.verify_resources().map_err(|e| vm_error(&e))?;
        Ok(Self {
            vm,
            boundary,
            failed: false,
            has_stream,
            registry,
            native_error,
        })
    }

    /// Register `function` as the native `name`, keeping the bounds already declared
    /// for it unless `wcet` or `wcmu_bytes` is given.
    ///
    /// A raised memory bound is checked against this VM's arena, and refused with
    /// `keleusma.VerifyError` if the arena cannot hold it, leaving the earlier
    /// registration in place. Declare large bounds when building the VM instead.
    #[pyo3(signature = (name, function, *, wcet = None, wcmu_bytes = None))]
    fn register(
        &mut self,
        name: &str,
        function: &Bound<'_, PyAny>,
        wcet: Option<u32>,
        wcmu_bytes: Option<u32>,
    ) -> PyResult<()> {
        if !function.is_callable() {
            return Err(PyTypeError::new_err(format!(
                "the native `{name}` must be callable"
            )));
        }
        let (old_wcet, old_wcmu) = self.bounds_of(name)?;
        self.apply_bounds(
            name,
            wcet.unwrap_or(old_wcet),
            wcmu_bytes.unwrap_or(old_wcmu),
        )?;
        if let Some(slot) = self.registry.borrow_mut().get_mut(name) {
            slot.callable = Some(function.clone().unbind());
        }
        Ok(())
    }

    /// Declare the worst-case time, in Keleusma's nominal cost units, and arena memory,
    /// in bytes, of one call of the native `name`.
    ///
    /// A memory bound this VM's arena cannot hold is refused with
    /// `keleusma.VerifyError`, and the earlier bounds stay in place.
    fn set_native_bounds(&mut self, name: &str, wcet: u32, wcmu_bytes: u32) -> PyResult<()> {
        self.bounds_of(name)?;
        self.apply_bounds(name, wcet, wcmu_bytes)
    }

    /// The bounds declared for the native `name`, as `(wcet, wcmu_bytes)`.
    fn native_bounds(&self, name: &str) -> PyResult<(u32, u32)> {
        self.bounds_of(name)
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        // `try_borrow`, because traversal must never panic. A registry borrowed at
        // that moment is mid-update, and skipping one traversal is harmless.
        if let Ok(registry) = self.registry.try_borrow() {
            for slot in registry.values() {
                if let Some(callable) = &slot.callable {
                    visit.call(callable)?;
                }
            }
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        if let Ok(mut registry) = self.registry.try_borrow_mut() {
            for slot in registry.values_mut() {
                slot.callable = None;
            }
        }
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
    ///
    /// `shared` is a writable buffer of exactly `vm.shared_data_bytes` bytes for a
    /// module that declares a shared `data` block, and must be omitted otherwise.
    #[pyo3(signature = (*args, shared = None))]
    fn call<'py>(
        &mut self,
        py: Python<'py>,
        args: &Bound<'py, PyTuple>,
        shared: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.check_usable()?;
        let mut lent = self.lend(shared)?;
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
        let outcome = self.vm.call_with_shared(lent.bytes(), &values);
        let state = self.state(py, outcome)?;
        lent.give_back(py)?;
        Ok(state)
    }

    /// Resume after a yield or a reset with `value`, converted by the type of the
    /// stream's parameter.
    ///
    /// Returns `Yielded`, `Finished` or `Reset`. Raises `keleusma.VMError` if there is
    /// nothing to resume, for example before `call` or after `Finished`.
    ///
    /// `shared` follows the same rule as in `call`.
    #[pyo3(signature = (value, *, shared = None))]
    fn resume<'py>(
        &mut self,
        py: Python<'py>,
        value: &Bound<'py, PyAny>,
        shared: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.check_usable()?;
        let mut lent = self.lend(shared)?;
        let ty = declared(&self.signature()?.resume, "resume")?;
        let input = to_value(value, ty)?;
        let outcome = self.vm.resume_with_shared(lent.bytes(), input);
        let state = self.state(py, outcome)?;
        lent.give_back(py)?;
        Ok(state)
    }

    /// The size in bytes of the module's shared `data` block, zero if it has none.
    #[getter]
    fn shared_data_bytes(&self) -> usize {
        self.vm.shared_data_bytes()
    }

    /// Drive the VM through Python's generator protocol, yielding each `Yielded` and
    /// `Reset` and returning the `Finished` value. Resume inputs come from `inputs`
    /// or from `send`. The semantics, including the meaning of `Reset`, are
    /// documented in `keleusma._stream`, which implements the generator, because a
    /// generator is far simpler to write and to read in Python than through PyO3.
    #[pyo3(signature = (*args, inputs = None, shared = None))]
    fn stream<'py>(
        slf: Bound<'py, Self>,
        args: &Bound<'py, PyTuple>,
        inputs: Option<Bound<'py, PyAny>>,
        shared: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let mut positional = vec![slf.into_any()];
        positional.extend(args.iter());
        let keywords = PyDict::new(py);
        keywords.set_item("inputs", inputs)?;
        keywords.set_item("shared", shared)?;
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
        // Refused here, before the VM runs, so a forgotten native leaves the VM usable.
        if let Some((name, _)) = self
            .registry
            .borrow()
            .iter()
            .find(|(_, slot)| slot.callable.is_none())
        {
            return Err(VMError::new_err(format!(
                "the native `{name}` is not registered; pass it in `natives=` or call \
                 `vm.register`"
            )));
        }
        Ok(())
    }

    /// Check `shared` against the module's shared block and copy it in. See [`Lent`].
    fn lend(&self, shared: Option<&Bound<'_, PyAny>>) -> PyResult<Lent> {
        let size = self.vm.shared_data_bytes();
        match (size, shared) {
            (0, None) => Ok(Lent::none()),
            (0, Some(_)) => Err(PyTypeError::new_err(
                "the module declares no shared `data` block, so `shared` must be omitted",
            )),
            (size, None) => Err(PyTypeError::new_err(format!(
                "the module declares a shared `data` block of {size} bytes; pass \
                 `shared=` a writable buffer of that size, such as a bytearray"
            ))),
            (size, Some(object)) => Lent::new(object, size),
        }
    }

    fn bounds_of(&self, name: &str) -> PyResult<(u32, u32)> {
        let registry = self.registry.borrow();
        let slot = registry.get(name).ok_or_else(|| {
            let names: Vec<String> = registry.keys().cloned().collect();
            unknown_native(name, &names)
        })?;
        Ok((slot.wcet, slot.wcmu_bytes))
    }

    /// Set the bounds of `name`, check the arena, and restore the earlier bounds if the
    /// arena cannot hold the new ones.
    fn apply_bounds(&mut self, name: &str, wcet: u32, wcmu_bytes: u32) -> PyResult<()> {
        let (old_wcet, old_wcmu) = self.bounds_of(name)?;
        self.vm
            .set_native_bounds(name, wcet, wcmu_bytes)
            .map_err(|e| vm_error(&e))?;
        if let Err(error) = self.vm.verify_resources() {
            self.vm
                .set_native_bounds(name, old_wcet, old_wcmu)
                .map_err(|e| vm_error(&e))?;
            let detail = match error {
                VmError::VerifyError(message) => message,
                other => format!("{other:?}"),
            };
            return Err(VerifyError::new_err(format!(
                "a memory bound of {wcmu_bytes} bytes for the native `{name}` does not fit \
                 this VM's arena ({detail}); declare it in `natives=` when building the VM"
            )));
        }
        if let Some(slot) = self.registry.borrow_mut().get_mut(name) {
            slot.wcet = wcet;
            slot.wcmu_bytes = wcmu_bytes;
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
                // A native's own exception reaches the caller unchanged.
                if let Some(raised) = self.native_error.borrow_mut().take() {
                    return Err(raised);
                }
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
