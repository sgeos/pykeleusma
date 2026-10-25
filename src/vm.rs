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
//! # Shared data
//!
//! Keleusma lends a module's shared `data` block to each call as a host-owned byte
//! buffer. The VM owns that buffer in Rust for its whole life, zeroed at construction.
//! A call given `shared=` syncs the Rust copy from the Python buffer, runs the VM, and
//! flushes the Rust copy back to the Python buffer. A call without it runs on the Rust
//! copy as it stands, so state persists across calls either way. The VM never lends
//! Python's memory to Keleusma, so whatever Python does to its own buffer during a call,
//! for example from a native, cannot affect the run, and the flush overwrites it. That
//! is a matter for the Python code, not for Keleusma.
//!
//! After a failed call the flush is skipped, so the Python buffer keeps its pre-call
//! contents while the Rust copy holds whatever the script wrote before failing. The VM
//! is then failed and refuses further use, so the Rust copy is not used again.
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

use keleusma::bytecode::{BlockType, Value};
use keleusma::vm::{DEFAULT_NATIVE_WCET, DEFAULT_NATIVE_WCMU_BYTES, VmError, VmState};
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{
    PyKeyError, PyMemoryError, PyOverflowError, PyRuntimeError, PyTypeError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyMapping, PyTuple};
use pyo3::{PyTraverseError, PyVisit};

use crate::compile::{Boundary, Module, SignedImage};
use crate::convert::{ReadContext, to_py, to_py_untyped, to_value, to_value_untyped};
use crate::libraries::Library;
use crate::natives::{self, ErrorSlot, Registry, Slot, closure};
use crate::owned_vm::{OwnedVm, OwnedVmError};
use crate::types::{Declared, Ty};

pyo3::import_exception!(keleusma._errors, VerifyError);
pyo3::import_exception!(keleusma._errors, VMError);
pyo3::import_exception!(keleusma._errors, LoadError);

/// The Python exception for a Keleusma VM error.
///
/// A verification failure passes the verifier's message through unchanged. Every
/// other error is described by its `Debug` form, because `VmError` implements `Debug`
/// but not `Display`.
pub fn vm_error(error: &VmError) -> PyErr {
    match error {
        VmError::VerifyError(message) => VerifyError::new_err(message.clone()),
        // Keleusma reports malformed bytes and a signature that verifies against no
        // trusted key as a load error.
        VmError::LoadError(load) => LoadError::new_err(format!("{load:?}")),
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

/// A Python buffer to sync the shared `data` block from before a call, and to flush it
/// to after.
///
/// Checked here so that an unsuitable buffer is refused before the VM runs.
/// A contiguous byte buffer of `size` bytes to read a shared block from, which,
/// unlike one to write to, may be read-only.
fn readable_buffer(object: &Bound<'_, PyAny>, size: usize) -> PyResult<PyBuffer<u8>> {
    let buffer = PyBuffer::<u8>::get(object).map_err(|_| {
        PyTypeError::new_err("`buffer` must support the buffer protocol with byte items")
    })?;
    if !buffer.is_c_contiguous() {
        return Err(PyValueError::new_err("`buffer` must be contiguous"));
    }
    if buffer.item_count() != size {
        return Err(PyValueError::new_err(format!(
            "`buffer` has {} bytes, but the module's shared `data` block has {size}",
            buffer.item_count()
        )));
    }
    Ok(buffer)
}

fn host_buffer(object: &Bound<'_, PyAny>, size: usize) -> PyResult<PyBuffer<u8>> {
    let buffer = PyBuffer::<u8>::get(object).map_err(|_| {
        PyTypeError::new_err(
            "`shared` must support the buffer protocol with byte items, such as a \
             bytearray, a memoryview of one, or a NumPy uint8 array",
        )
    })?;
    if buffer.readonly() {
        return Err(PyTypeError::new_err(
            "`shared` must be writable, because the script's changes are flushed to it",
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
    Ok(buffer)
}

/// The shared `data` block's declared type, checked against Keleusma's own layout
/// table, or why the binding cannot convert the block by its fields.
///
/// The binding lays the block out from the declared field types, as a packed struct.
/// Keleusma records each shared slot's offset when it compiles the module, so every
/// offset is compared with the binding's, and any difference refuses typed access
/// rather than reading a field at the wrong place.
fn shared_type(module: &Module) -> Result<Ty, String> {
    let declared = match &module.boundary().shared {
        None => {
            return Err(
                "the module declares no shared `data` block, or was loaded from bytes, \
                 which carry no declarations"
                    .to_string(),
            );
        }
        Some(Err(unsupported)) => {
            return Err(format!("the shared block has {}", unsupported.0));
        }
        Some(Ok(ty)) => ty.clone(),
    };
    let widths = module.boundary().widths;
    let layout = declared.layout();
    let Some(data) = module.inner().data_layout.as_ref() else {
        return Err("Keleusma records no data layout for the module".to_string());
    };
    let Ty::Struct {
        name: block,
        fields,
    } = &declared
    else {
        return Err("the shared block is not a struct of fields".to_string());
    };
    // Keleusma names each shared slot `block.field`, and expands an array field into
    // one slot per element, `block.field[i]`. Each recorded offset must equal the
    // offset the binding computes for that name.
    let shared_slots = data
        .slots
        .iter()
        .filter(|slot| matches!(slot.visibility, keleusma::bytecode::SlotVisibility::Shared));
    let mut checked = 0usize;
    for (slot, recorded) in shared_slots.zip(&data.shared_layout) {
        let expected = slot
            .name
            .strip_prefix(block.as_str())
            .and_then(|rest| rest.strip_prefix('.'))
            .and_then(|rest| shared_slot_offset(rest, fields, &layout, widths));
        if expected != crate::compat::shared_slot_offset(recorded) {
            return Err(format!(
                "the slot `{}` lies at offset {} in Keleusma's layout, not {expected:?}",
                slot.name, recorded.offset
            ));
        }
        checked += 1;
    }
    if checked != data.shared_layout.len() || checked == 0 {
        return Err(format!(
            "Keleusma records {} shared slot(s), of which {checked} were matched",
            data.shared_layout.len()
        ));
    }
    Ok(declared)
}

/// The new module's private slot values for a hot swap: the field values `private`
/// names, converted by the module's declared private fields, and `()` for every slot
/// it does not name, the state a fresh VM starts in.
///
/// Keleusma lists one private slot per field, and one per element of an array field,
/// named `block.field` or `block.field[i]`, and takes the values in that order.
fn private_values(module: &Module, private: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<Value>> {
    let fresh = OwnedVm::fresh_private_data(module.inner());
    let Some(private) = private else {
        return Ok(fresh);
    };
    let values = private
        .cast::<PyMapping>()
        .map_err(|_| PyTypeError::new_err("`private` must be a mapping of field names"))?;
    let Some(Ok(Ty::Struct {
        name: block,
        fields,
    })) = &module.boundary().private
    else {
        return Err(PyTypeError::new_err(
            "`private` needs a module compiled from source that declares a private `data` \
             block with fields of supported types",
        ));
    };
    for key in values.keys()?.iter() {
        let key: String = key.extract()?;
        if !fields.iter().any(|(n, _)| *n == key) {
            let known: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
            return Err(PyKeyError::new_err(format!(
                "the private block `{block}` has no field `{key}`; its fields are {}",
                known.join(", ")
            )));
        }
    }
    let slots = module
        .inner()
        .data_layout
        .as_ref()
        .map(|d| d.slots.clone())
        .unwrap_or_default();
    let private_slots = slots
        .iter()
        .filter(|slot| matches!(slot.visibility, keleusma::bytecode::SlotVisibility::Private));
    let mut initial = Vec::with_capacity(fresh.len());
    for slot in private_slots {
        let rest = slot
            .name
            .strip_prefix(block.as_str())
            .and_then(|rest| rest.strip_prefix('.'))
            .ok_or_else(|| VMError::new_err(format!("an unexpected slot `{}`", slot.name)))?;
        let (field, element) = match rest.split_once('[') {
            Some((field, index)) => {
                let index = index
                    .strip_suffix(']')
                    .and_then(|i| i.parse::<usize>().ok())
                    .ok_or_else(|| VMError::new_err(format!("an unexpected slot `{rest}`")))?;
                (field, Some(index))
            }
            None => (rest, None),
        };
        let field_ty = fields
            .iter()
            .find(|(n, _)| n == field)
            .map(|(_, t)| t)
            .ok_or_else(|| VMError::new_err(format!("an undeclared slot `{}`", slot.name)))?;
        let value = match values.get_item(field) {
            Err(_) => Value::Unit,
            // `None` is a slot not yet written, as `private_data` reports it.
            Ok(object) if object.is_none() => Value::Unit,
            Ok(object) => match (element, field_ty) {
                (None, ty) if is_scalar(ty) => to_value(&object, ty)?,
                (None, ty) => {
                    // KELEUSMA-0.2.2: re-check on an upgrade whether composite private
                    // values survive `initial_data`, and if so convert them here.
                    return Err(PyTypeError::new_err(format!(
                        "the private field `{field}` is {}, a composite; Keleusma 0.2.2 \
                         clears the composite body pool after seeding the slots, so only \
                         scalar fields and arrays of scalars can be seeded at a swap",
                        ty.name()
                    )));
                }
                (Some(i), Ty::Array(element_ty, count)) if is_scalar(element_ty) => {
                    let items: Vec<Bound<'_, PyAny>> =
                        object.try_iter()?.collect::<PyResult<_>>()?;
                    if items.len() != *count {
                        return Err(PyTypeError::new_err(format!(
                            "expected {}, got a sequence of length {}",
                            field_ty.name(),
                            items.len()
                        )));
                    }
                    if items[i].is_none() {
                        Value::Unit
                    } else {
                        to_value(&items[i], element_ty)?
                    }
                }
                _ => return Err(VMError::new_err(format!("an unexpected slot `{rest}`"))),
            },
        };
        initial.push(value);
    }
    if initial.len() != fresh.len() {
        return Err(VMError::new_err(format!(
            "{} private slot value(s) for {} slot(s)",
            initial.len(),
            fresh.len()
        )));
    }
    Ok(initial)
}

/// Whether a value of `ty` occupies a private slot directly, as a scalar.
fn is_scalar(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Unit | Ty::Bool | Ty::Byte | Ty::Word | Ty::Fixed(_) | Ty::Float
    )
}

/// The binding's offset for the shared slot named `field` or `field[i]` within the
/// block, or `None` for a name it cannot place.
fn shared_slot_offset(
    name: &str,
    fields: &[(String, Ty)],
    layout: &keleusma::value_layout::LayoutDescriptor,
    widths: crate::convert::Widths,
) -> Option<usize> {
    let (field, element) = match name.split_once('[') {
        Some((field, rest)) => (field, Some(rest.strip_suffix(']')?.parse::<usize>().ok()?)),
        None => (name, None),
    };
    let index = fields.iter().position(|(n, _)| n == field)?;
    let base = widths.offset_of(layout, index)?;
    match (element, &fields[index].1) {
        (None, _) => Some(base),
        (Some(i), Ty::Array(element_ty, count)) if i < *count => {
            base.checked_add(i.checked_mul(widths.size_of(&element_ty.layout()))?)
        }
        _ => None,
    }
}

/// Refuse a module whose declaration of a library native differs from the library's.
///
/// Keleusma's compiler takes a script's `use math::sqrt(...) -> ...` line on trust and
/// does not compare it with the library's real signature. A library native is Rust
/// code that builds its result by its own signature, while the binding converts the
/// result by the script's declared type, so a mismatch reads a value as a type it does
/// not have. A probe of `shell::run` declared with its tuple reversed returned wrong
/// values silently, and Keleusma 0.2.2 is known to crash when a body is decoded with
/// a mismatched type. Only natives a library serves are checked, because the binding
/// converts a host callable's values by the script's declaration on both sides. A bare
/// `use` line and a module without declarations convert untyped, by each value's own
/// tag, and are not checked.
///
/// KELEUSMA-0.2.2: no check was found on the 0.2.3 branch. If Keleusma adds one, as
/// `pykeleusma-20261025-library-declarations` asks, this may go.
fn check_library_declarations(
    libraries: &[Library],
    boundary: &Boundary,
    names: &[String],
    host_served: impl Fn(&str) -> bool,
) -> PyResult<()> {
    for library in libraries {
        let provided = library.native_names();
        let declarations = library.declarations().ok_or_else(|| {
            VerifyError::new_err("the library's published signatures do not parse")
        })?;
        for name in names
            .iter()
            .filter(|n| provided.contains(n) && !host_served(n))
        {
            let Some(script) = boundary
                .natives
                .get(name)
                .and_then(|d| d.signature.as_ref())
            else {
                continue;
            };
            let library_signature = declarations.get(name).and_then(|d| d.signature.as_ref());
            if library_signature != Some(script) {
                let expected = library
                    .declaration_line(name)
                    .unwrap_or("its published one");
                return Err(VerifyError::new_err(format!(
                    "the script declares the library native `{name}` with a signature that \
                     differs from the library's, `{expected}`; declare it as the library \
                     does, as `keleusma.library_signatures` gives it, or pass a host \
                     callable for it"
                )));
            }
        }
    }
    Ok(())
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
/// The `use external` natives among `names`, each with its attested calls per
/// iteration, checked against what the host passed.
///
/// An external native needs a callable and `max_invocations`, because Keleusma
/// records the attestation in place of a per-call bound. `max_invocations` for any
/// other native is refused, because Keleusma would ignore it.
fn external_natives(
    boundary: &Boundary,
    names: &[String],
    slots: &BTreeMap<String, Slot>,
) -> PyResult<Vec<(String, u32)>> {
    let mut external = Vec::new();
    for name in names {
        let is_external = boundary.natives.get(name).is_some_and(|d| d.external);
        let slot = slots.get(name);
        let attested = slot.and_then(|s| s.max_invocations);
        match (is_external, attested) {
            (true, Some(n)) if slot.is_some_and(|s| s.callable.is_some()) => {
                external.push((name.clone(), n));
            }
            (true, _) => {
                return Err(external_needs_attestation(name));
            }
            (false, Some(_)) => {
                return Err(PyTypeError::new_err(format!(
                    "`max_invocations` applies only to a `use external` native, and \
                     `{name}` is not one"
                )));
            }
            (false, None) => {}
        }
    }
    if external.len() > crate::external::MAX_EXTERNAL {
        return Err(VerifyError::new_err(format!(
            "the module declares {} `use external` natives, more than the {} the binding \
             can serve",
            external.len(),
            crate::external::MAX_EXTERNAL
        )));
    }
    Ok(external)
}

/// The error for a `use external` native passed without what it needs.
fn external_needs_attestation(name: &str) -> PyErr {
    PyTypeError::new_err(format!(
        "the native `{name}` is declared `use external`, so pass it as \
         keleusma.Native(function, max_invocations=n), where n bounds its calls per \
         stream iteration"
    ))
}

/// A native's callable and the bounds declared with it: time, memory, and for a
/// `use external` native the calls per iteration.
type NativeParts = (Py<PyAny>, Option<u32>, Option<u32>, Option<u32>);

fn native_parts(value: &Bound<'_, PyAny>) -> PyResult<NativeParts> {
    let native_class = value.py().import("keleusma._native")?.getattr("Native")?;
    if value.is_instance(&native_class)? {
        let function = value.getattr("function")?;
        if !function.is_callable() {
            return Err(PyTypeError::new_err("a Native's function must be callable"));
        }
        let wcet: Option<u32> = value.getattr("wcet")?.extract()?;
        let wcmu_bytes: Option<u32> = value.getattr("wcmu_bytes")?.extract()?;
        let max_invocations: Option<u32> = value.getattr("max_invocations")?.extract()?;
        return Ok((function.unbind(), wcet, wcmu_bytes, max_invocations));
    }
    if value.is_callable() {
        return Ok((value.clone().unbind(), None, None, None));
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
    /// The module's shared `data` block, owned here for the VM's whole life and lent
    /// to Keleusma on every call. See "Shared data" in the module documentation.
    shared: Vec<u8>,
    /// The bundled libraries registered at construction, whose natives persist across
    /// a hot swap like every other registration.
    libraries: Vec<Library>,
    /// Whether the VM is suspended at a yield, where a hot swap is not allowed.
    suspended: bool,
    /// Whether the last state was `Reset`, after which a resume starts the next
    /// iteration of the stream.
    at_reset: bool,
    /// The shared block's declared type, or why it cannot be converted by fields.
    shared_type: Result<Ty, String>,
    /// The running module, which a cost-model analysis reads, since Keleusma keeps
    /// its own copy private.
    module: keleusma::bytecode::Module,
    /// Each `use external` native's index in the trampoline table.
    external_index: BTreeMap<String, usize>,
}

#[pymethods]
impl VM {
    #[new]
    #[pyo3(signature = (
        module, *, argument_bytes = crate::DEFAULT_ARGUMENT_BYTES, natives = None,
        libraries = None,
    ))]
    fn new(
        module: PyRef<'_, Module>,
        argument_bytes: usize,
        natives: Option<&Bound<'_, PyAny>>,
        libraries: Option<Vec<String>>,
    ) -> PyResult<Self> {
        let libraries = libraries
            .unwrap_or_default()
            .iter()
            .map(|name| {
                Library::named(name).ok_or_else(|| {
                    PyValueError::new_err(format!(
                        "unknown library `{name}`; the libraries are math, audio and shell"
                    ))
                })
            })
            .collect::<PyResult<Vec<Library>>>()?;
        let names = module.inner().native_names.clone();
        module.check_signature_route()?;
        let boundary = module.boundary().clone();
        // Every native the module names gets a slot, with Keleusma's default bounds
        // unless the host declares its own, so the arena is sized for all of them.
        let mut slots: BTreeMap<String, Slot> = names
            .iter()
            .map(|name| {
                let slot = Slot {
                    callable: None,
                    wcet: DEFAULT_NATIVE_WCET,
                    wcmu_bytes: DEFAULT_NATIVE_WCMU_BYTES,
                    library: false,
                    max_invocations: None,
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
                let (callable, wcet, wcmu_bytes, max_invocations) = native_parts(&value)?;
                slot.callable = Some(callable);
                slot.wcet = wcet.unwrap_or(DEFAULT_NATIVE_WCET);
                slot.wcmu_bytes = wcmu_bytes.unwrap_or(DEFAULT_NATIVE_WCMU_BYTES);
                slot.max_invocations = max_invocations;
            }
        }
        let external_names = external_natives(&boundary, &names, &slots)?;
        check_library_declarations(&libraries, &boundary, &names, |n| {
            slots.get(n).is_some_and(|s| s.callable.is_some())
        })?;
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
        let mut vm = match module.signed() {
            // A module loaded from signed bytes is built through Keleusma's signed
            // loader, which verifies the signature again and keeps the keys as the
            // VM's trust matrix.
            Some(SignedImage {
                bytes,
                encrypted: false,
                keys,
            }) => OwnedVm::new_signed(module.inner(), bytes, keys, argument_bytes, &native_wcmu),
            // A decrypted module was verified while decrypting. It is built normally
            // and its keys become the trust matrix, as in Keleusma's encrypted loader.
            Some(SignedImage {
                encrypted: true,
                keys,
                ..
            }) => {
                OwnedVm::new(module.inner().clone(), argument_bytes, &native_wcmu).map(|mut vm| {
                    for key in keys {
                        vm.register_verifying_key(*key);
                    }
                    vm
                })
            }
            None => OwnedVm::new(module.inner().clone(), argument_bytes, &native_wcmu),
        }
        .map_err(construction_error)?;
        let registry: Registry = Rc::new(RefCell::new(slots));
        let native_error: ErrorSlot = Rc::new(RefCell::new(None));
        // A `use external` native is served through a trampoline, with the host's
        // attested call count, and every other native as a verified closure.
        let table = vm.external_table();
        table.borrow_mut().widths = Some(boundary.widths);
        let mut external_index = BTreeMap::new();
        for (name, _, _) in &bounds {
            let decl = boundary.natives.get(name).cloned();
            let func = closure(name.clone(), decl, registry.clone(), native_error.clone());
            match external_names.iter().find(|(n, _)| n == name) {
                Some((_, max_invocations)) => {
                    let index = table.borrow().closures.len();
                    table.borrow_mut().closures.push(Rc::new(func));
                    vm.register_external_native(name, index, *max_invocations);
                    external_index.insert(name.clone(), index);
                }
                None => vm.register_native(name, func),
            }
        }
        // A library replaces the binding's closure for each native it provides, except
        // where the host passed its own callable for that name, which is restored.
        for &library in &libraries {
            vm.register_library(library);
            for name in library.native_names() {
                let host_supplied = registry
                    .borrow()
                    .get(&name)
                    .map(|slot| slot.callable.is_some());
                match host_supplied {
                    Some(true) => {
                        let decl = boundary.natives.get(&name).cloned();
                        let func =
                            closure(name.clone(), decl, registry.clone(), native_error.clone());
                        vm.register_native(&name, func);
                    }
                    Some(false) => {
                        if let Some(slot) = registry.borrow_mut().get_mut(&name) {
                            slot.library = true;
                        }
                    }
                    None => {}
                }
            }
        }
        for (name, wcet, wcmu_bytes) in &bounds {
            vm.set_native_bounds(name, *wcet, *wcmu_bytes)
                .map_err(|e| vm_error(&e))?;
        }
        // The arena was sized with these bounds, so this confirms the sizing rather
        // than expecting to fail.
        vm.verify_resources().map_err(|e| vm_error(&e))?;
        // Zeroed, so a script reads defined bytes before the host writes any.
        let shared = vec![0u8; vm.shared_data_bytes()];
        let shared_type = shared_type(&module);
        let analysed = module.inner().clone();
        Ok(Self {
            module: analysed,
            external_index,
            vm,
            boundary,
            shared_type,
            failed: false,
            has_stream,
            registry,
            native_error,
            shared,
            libraries,
            suspended: false,
            at_reset: false,
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
        let was_library = {
            let mut registry = self.registry.borrow_mut();
            let slot = registry.get_mut(name);
            let was_library = slot.as_ref().is_some_and(|slot| slot.library);
            if let Some(slot) = slot {
                slot.callable = Some(function.clone().unbind());
                slot.library = false;
            }
            was_library
        };
        // A library's implementation replaced the binding's closure, so a Python
        // callable for the same name needs the closure back.
        if was_library {
            let decl = self.boundary.natives.get(name).cloned();
            let func = closure(
                name.to_string(),
                decl,
                self.registry.clone(),
                self.native_error.clone(),
            );
            self.vm.register_native(name, func);
        }
        Ok(())
    }

    /// Replace the running module with `module`, as the book's chapter 37 describes.
    ///
    /// Allowed only at an iteration boundary: before the first call, after a `Reset` or
    /// a `Finished`, or after `reset()`. Raises `keleusma.VMError` while the VM is
    /// suspended at a yield. Keleusma verifies the new module against this VM's arena
    /// first, raising `keleusma.VerifyError` and keeping the old module if it does not
    /// fit, and it requires the same data segment schema as the old module.
    ///
    /// After the swap the new module is driven from `call`. Its private data starts as
    /// `()`, as in a fresh VM. Native registrations persist, and a native the new module
    /// names that is not yet registered must be registered before the next call.
    #[pyo3(signature = (module, *, private = None))]
    fn replace_module(
        &mut self,
        module: PyRef<'_, Module>,
        private: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        if self.suspended {
            return Err(VMError::new_err(
                "a hot swap is allowed only at an iteration boundary, after `Reset` or \
                 `Finished`, not while the script is suspended at a yield",
            ));
        }
        module.check_signature_route()?;
        let boundary = module.boundary().clone();
        check_library_declarations(
            &self.libraries,
            &boundary,
            &module.inner().native_names,
            |n| {
                self.registry
                    .borrow()
                    .get(n)
                    .is_some_and(|s| s.callable.is_some())
            },
        )?;
        // Keleusma keeps a native's classification across a swap, so a native must be
        // external in the new module exactly when it was registered as external.
        for name in &module.inner().native_names {
            let external = boundary.natives.get(name).is_some_and(|d| d.external);
            if external != self.external_index.contains_key(name) {
                return Err(VerifyError::new_err(format!(
                    "the native `{name}` is {} in the new module but was {} in this VM, \
                     and Keleusma keeps a native's classification across a swap; build a \
                     new VM from the module instead",
                    if external {
                        "`use external`"
                    } else {
                        "not external"
                    },
                    if external { "not external" } else { "external" },
                )));
            }
        }
        let native_wcmu: Vec<u32> = {
            let registry = self.registry.borrow();
            module
                .inner()
                .native_names
                .iter()
                .map(|name| {
                    registry
                        .get(name)
                        .map_or(DEFAULT_NATIVE_WCMU_BYTES, |s| s.wcmu_bytes)
                })
                .collect()
        };
        // Converted before anything changes, so a refused value leaves the old module.
        let initial = private_values(&module, private)?;
        match module.signed() {
            // Signed bytes are verified against this VM's trust matrix, the keys it
            // was built with and those added by `register_verifying_key`. For an
            // encrypted module the signature covers the encrypted bytes, which were
            // kept for this check, so no decryption key is needed again.
            Some(SignedImage { bytes, .. }) => {
                self.vm
                    .replace_module_signed(module.inner(), bytes, &native_wcmu, initial)
            }
            None => self
                .vm
                .replace_module(module.inner().clone(), &native_wcmu, initial),
        }
        .map_err(|e| match e {
            // Keleusma's message points to its unchecked swap, which the binding does
            // not expose, because it skips the check that keeps shared data meaningful.
            VmError::VerifyError(message) if message.starts_with("schema mismatch") => {
                VerifyError::new_err(format!(
                    "{message}; pykeleusma does not expose the unchecked swap, so build \
                     a new VM from the module instead"
                ))
            }
            other => vm_error(&other),
        })?;
        // The new module may name natives the old one did not. Each gets a slot,
        // satisfied by a registered library where one provides it. Every native not
        // served by a library gets a fresh closure, because a closure converts by the
        // declaration of the module it was built for, and the new module may declare a
        // native differently. A stale declaration would build a value of the old type
        // where the new module expects the new one.
        let provided: Vec<String> = self
            .libraries
            .iter()
            .flat_map(|l| l.native_names())
            .collect();
        for name in &module.inner().native_names {
            let existing = self
                .registry
                .borrow()
                .get(name)
                .map(|slot| slot.library && slot.callable.is_none());
            let library = match existing {
                Some(served_by_library) => served_by_library,
                None => {
                    let library = provided.contains(name);
                    self.registry.borrow_mut().insert(
                        name.clone(),
                        Slot {
                            callable: None,
                            wcet: DEFAULT_NATIVE_WCET,
                            wcmu_bytes: DEFAULT_NATIVE_WCMU_BYTES,
                            library,
                            max_invocations: None,
                        },
                    );
                    library
                }
            };
            if !library {
                let decl = boundary.natives.get(name).cloned();
                let func = closure(
                    name.clone(),
                    decl,
                    self.registry.clone(),
                    self.native_error.clone(),
                );
                // An external native keeps its trampoline, whose closure is replaced.
                match self.external_index.get(name) {
                    Some(&index) => {
                        let table = self.vm.external_table();
                        let mut table = table.borrow_mut();
                        table.closures[index] = Rc::new(func);
                        table.widths = Some(boundary.widths);
                    }
                    None => self.vm.register_native(name, func),
                }
            }
        }
        self.has_stream = module
            .inner()
            .chunks
            .iter()
            .any(|chunk| chunk.block_type == BlockType::Stream);
        let size = self.vm.shared_data_bytes();
        if self.shared.len() != size {
            self.shared = vec![0u8; size];
        }
        self.shared_type = shared_type(&module);
        self.module = module.inner().clone();
        // The new module is driven from `call`, not from the old module's `Reset`.
        self.at_reset = false;
        self.boundary = boundary;
        self.failed = false;
        // The arena was checked against the new module above, so this confirms it with
        // Keleusma's own analysis of the registered natives' bounds.
        self.vm.verify_resources().map_err(|e| vm_error(&e))?;
        Ok(())
    }

    /// Trust `key`, a 32-byte Ed25519 public key, for signed modules swapped in later
    /// with `replace_module`, as the book's chapter 36 describes for a host that boots
    /// from an unsigned baseline and accepts only signed updates.
    fn register_verifying_key(&mut self, key: &[u8]) -> PyResult<()> {
        self.vm
            .register_verifying_key(crate::crypto::verifying_key(key)?);
        Ok(())
    }

    /// Recover after a runtime error, as the book's chapter 34 describes.
    ///
    /// Clears the operand stack, the call frames and the arena, keeps the private and
    /// shared data, and makes the VM usable again, so the next `call` starts the entry
    /// point afresh. The host attests by calling it that it has inspected the error and
    /// decided to retry.
    fn reset(&mut self) {
        self.vm.reset_after_error();
        self.native_error.borrow_mut().take();
        self.failed = false;
        self.suspended = false;
        self.at_reset = false;
    }

    /// Declare the worst-case time, in nominal cycles, and arena memory,
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

    /// The worst-case execution time of one iteration of a `loop main`, in nominal
    /// cycles, or `None` for a module without a stream.
    ///
    /// Nominal cycles are the per-instruction costs of Keleusma's nominal cost model,
    /// not the cycles of any particular processor, and not seconds. The figure covers
    /// the script and the declared bounds of any natives, never Python or the
    /// binding's own conversions.
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

    /// The worst-case execution time of one iteration of a `loop main` under a cost
    /// model of the host's, or `None` for a module without a stream.
    ///
    /// `op_cycles` maps opcode names, the names in `keleusma.OP_NAMES`, to a cost per
    /// execution, and every opcode it does not name keeps its nominal cost.
    /// `text_byte_cycles` is the cost per byte of text work, nominal if omitted. This
    /// is how a host applies a calibrated or measured model, such as one generated by
    /// Keleusma's benchmark crate for a particular processor, in that model's units.
    /// The native bounds are the declared ones, as for `wcet_per_iteration`.
    #[pyo3(signature = (op_cycles, *, text_byte_cycles = None))]
    fn wcet_per_iteration_with(
        &self,
        op_cycles: BTreeMap<String, u32>,
        text_byte_cycles: Option<u32>,
    ) -> PyResult<Option<u32>> {
        if let Some(unknown) = op_cycles
            .keys()
            .find(|name| !crate::compat::OP_NAMES.contains(&name.as_str()))
        {
            return Err(PyValueError::new_err(format!(
                "`{unknown}` is not a Keleusma opcode; the names are in keleusma.OP_NAMES"
            )));
        }
        if !self.has_stream {
            return Ok(None);
        }
        // The declared bounds of each native, in the order the module names them,
        // which is the order Keleusma's analysis indexes them by.
        let bounds: Vec<keleusma::verify::NativeIterationBound> = {
            let registry = self.registry.borrow();
            self.module
                .native_names
                .iter()
                .map(|name| {
                    let slot = registry.get(name);
                    keleusma::verify::NativeIterationBound {
                        per_call_wcmu_bytes: slot
                            .map_or(DEFAULT_NATIVE_WCMU_BYTES, |s| s.wcmu_bytes),
                        per_call_wcet_cycles: slot.map_or(DEFAULT_NATIVE_WCET, |s| s.wcet),
                        max_invocations: slot.and_then(|s| s.max_invocations),
                    }
                })
                .collect()
        };
        let stream_wcet = |table: BTreeMap<String, u32>, text: Option<u32>| {
            crate::compat::with_cost_table(table, text, |model| {
                keleusma::verify::module_wcet_with_bounds(&self.module, &bounds, model)
            })
            .map(|per_chunk| {
                self.module
                    .chunks
                    .iter()
                    .zip(per_chunk)
                    .filter(|(chunk, _)| chunk.block_type == BlockType::Stream)
                    .map(|(_, wcet)| wcet)
                    .max()
                    .unwrap_or(0)
            })
            .map_err(|e| VerifyError::new_err(format!("{}: {}", e.chunk_name, e.message)))
        };
        // The nominal model through this path must give Keleusma's own figure, or the
        // bounds were assembled differently from Keleusma's and the result is refused.
        let nominal = stream_wcet(BTreeMap::new(), None)?;
        let reference = self.vm.wcet_per_iteration().map_err(|e| vm_error(&e))?;
        if nominal != reference {
            return Err(VMError::new_err(format!(
                "the binding's nominal figure {nominal} differs from Keleusma's {reference}, \
                 so a custom cost model cannot be applied faithfully"
            )));
        }
        stream_wcet(op_cycles, text_byte_cycles).map(Some)
    }

    /// An upper bound on the most arena memory this VM has used at once, in bytes.
    ///
    /// The arena includes a margin for a shortfall in Keleusma 0.2.2's own sizing,
    /// which was measured rather than proven. Comparing this figure with
    /// `arena_capacity` after running a host's own scripts shows how much of that
    /// margin they use. Running out raises `keleusma.VMError` and never crashes.
    #[getter]
    fn arena_peak_bytes(&self) -> usize {
        self.vm.arena_peak_bytes()
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
    /// `shared`, for a module that declares a shared `data` block, is a writable
    /// buffer of exactly `vm.shared_data_bytes` bytes. The VM syncs its own copy of the
    /// block from it before running and flushes the copy back to it afterwards.
    /// Without it, the VM runs on its own copy as it stands.
    #[pyo3(signature = (*args, shared = None))]
    fn call<'py>(
        &mut self,
        py: Python<'py>,
        args: &Bound<'py, PyTuple>,
        shared: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.check_usable()?;
        let values = match &self.boundary.signature {
            Some(signature) => {
                if args.len() != signature.params.len() {
                    return Err(PyTypeError::new_err(format!(
                        "main takes {} argument(s), got {}",
                        signature.params.len(),
                        args.len()
                    )));
                }
                args.iter()
                    .zip(&signature.params)
                    .enumerate()
                    .map(|(index, (arg, param))| {
                        let ty = declared(param, &format!("argument {}", index + 1))?;
                        to_value(&arg, ty)
                    })
                    .collect::<PyResult<Vec<_>>>()?
            }
            // A module loaded from bytes has no declared types, so its arguments
            // convert as untyped scalars, and Keleusma checks their count and tags.
            None => args
                .iter()
                .map(|arg| to_value_untyped(&arg))
                .collect::<PyResult<Vec<_>>>()?,
        };
        let host = self.sync_shared(shared)?;
        let outcome = self.vm.call_with_shared(&mut self.shared, &values);
        let state = self.state(py, outcome)?;
        self.flush_shared(py, host)?;
        Ok(state)
    }

    /// Advance a stream to its next yield with `value`, in one call.
    ///
    /// A Keleusma 0.2.2 iteration of a `loop main` is a yield and then a `Reset`, so
    /// driving one iteration per frame takes two resumes, one completing the previous
    /// iteration and one starting the next. `step` does both with the same `value`,
    /// and starts the stream with `call(value)` when nothing is running. It returns the
    /// state the last resume or call returned, which is `Yielded` for the next yield,
    /// or `Finished`. A stream that yields more than once per iteration returns at its
    /// next yield without a `Reset` in between.
    ///
    /// `shared` follows the same rule as in `call`, and is lent to each resume.
    #[pyo3(signature = (value, *, shared = None))]
    fn step<'py>(
        &mut self,
        py: Python<'py>,
        value: &Bound<'py, PyAny>,
        shared: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if self.suspended {
            let state = self.resume(py, value, shared)?;
            if !self.at_reset {
                return Ok(state);
            }
        }
        if self.at_reset {
            return self.resume(py, value, shared);
        }
        let args = PyTuple::new(py, [value])?;
        self.call(py, &args, shared)
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
        let input = match &self.boundary.signature {
            Some(signature) => to_value(value, declared(&signature.resume, "resume")?)?,
            None => to_value_untyped(value)?,
        };
        let host = self.sync_shared(shared)?;
        let outcome = self.vm.resume_with_shared(&mut self.shared, input);
        let state = self.state(py, outcome)?;
        self.flush_shared(py, host)?;
        Ok(state)
    }

    /// The size in bytes of the module's shared `data` block, zero if it has none.
    #[getter]
    fn shared_data_bytes(&self) -> usize {
        self.shared.len()
    }

    /// A copy of the VM's own shared `data` block as it stands.
    #[getter]
    fn shared_data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.shared)
    }

    /// The private `data` block's fields as a `dict`, converted by their declared
    /// types, with `None` for a field, or an array element, not yet written, since a
    /// Keleusma 0.2.2 private slot starts as `()`.
    ///
    /// Passing the result, or part of it, as `replace_module(module, private=...)`
    /// carries the state of scalar fields across a hot swap. Read between runs.
    #[getter]
    fn private_data<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let Some(Ok(Ty::Struct {
            name: block,
            fields,
        })) = &self.boundary.private
        else {
            return Err(PyTypeError::new_err(
                "`private_data` needs a module compiled from source that declares a private \
                 `data` block with fields of supported types",
            ));
        };
        let names: Vec<String> = self
            .module
            .data_layout
            .as_ref()
            .map(|d| {
                d.slots
                    .iter()
                    .filter(|s| matches!(s.visibility, keleusma::bytecode::SlotVisibility::Private))
                    .map(|s| s.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let slots = self
            .vm
            .private_slots(names.len())
            .ok_or_else(|| VMError::new_err("the arena does not hold the private slots"))?;
        let none = |_: usize| None;
        let ctx = ReadContext {
            arena: self.vm.arena(),
            widths: self.boundary.widths,
            opaque: &none,
        };
        let convert = |value: &Value, ty: &Ty| -> PyResult<Bound<'py, PyAny>> {
            match value {
                Value::Unit if *ty != Ty::Unit => Ok(py.None().into_bound(py)),
                _ => to_py(py, value, ty, &ctx),
            }
        };
        let result = PyDict::new(py);
        for (field, ty) in fields {
            let prefix = format!("{block}.{field}");
            let first = names.iter().position(|n| *n == format!("{prefix}[0]"));
            let value = match ty {
                // KELEUSMA-0.2.2: a whole-array assignment stores the array in the
                // first element's slot and leaves the others `()`. That array is read
                // whole, and an epoch check refuses it if a `Reset` reclaimed it.
                Ty::Array(..) if first.is_some_and(|i| matches!(slots[i], Value::Array(_))) => {
                    let index = first.unwrap_or_default();
                    to_py(py, &slots[index], ty, &ctx)?
                }
                Ty::Array(element, count) => {
                    let mut items = Vec::with_capacity(*count);
                    for index in 0..*count {
                        let slot = format!("{prefix}[{index}]");
                        let position = names
                            .iter()
                            .position(|n| *n == slot)
                            .ok_or_else(|| VMError::new_err(format!("no private slot `{slot}`")))?;
                        items.push(convert(&slots[position], element)?);
                    }
                    pyo3::types::PyList::new(py, items)?.into_any()
                }
                _ => {
                    let position = names
                        .iter()
                        .position(|n| *n == prefix)
                        .ok_or_else(|| VMError::new_err(format!("no private slot `{prefix}`")))?;
                    convert(&slots[position], ty)?
                }
            };
            result.set_item(field, value)?;
        }
        Ok(result.into_any())
    }

    /// The shared `data` block's fields as a `dict`, converted by their declared
    /// types, read from the VM's own block or from `buffer`, a block-sized buffer.
    #[pyo3(signature = (buffer = None))]
    fn read_shared<'py>(
        &self,
        py: Python<'py>,
        buffer: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let ty = self.typed_shared()?;
        let bytes = match buffer {
            Some(object) => {
                let buffer = readable_buffer(object, self.shared.len())?;
                buffer.to_vec(py)?
            }
            None => self.shared.clone(),
        };
        let none = |_: usize| None;
        let ctx = ReadContext {
            arena: self.vm.arena(),
            widths: self.boundary.widths,
            opaque: &none,
        };
        crate::convert::read_flat_value(py, &bytes, ty, &ctx)
    }

    /// Write the fields named in `values` into the shared `data` block, converting
    /// each by its declared type, in the VM's own block or in `buffer`, a writable
    /// block-sized buffer. Fields not named keep their bytes. Every value is
    /// converted before any byte is written, so a refused value changes nothing.
    #[pyo3(signature = (values, buffer = None))]
    fn write_shared(
        &mut self,
        values: &Bound<'_, PyAny>,
        buffer: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let py = values.py();
        let ty = self.typed_shared()?.clone();
        let Ty::Struct { name, fields } = &ty else {
            return Err(VMError::new_err(
                "the shared block is not a struct of fields",
            ));
        };
        let values = values
            .cast::<PyMapping>()
            .map_err(|_| PyTypeError::new_err("`values` must be a mapping of field names"))?;
        let host = buffer
            .map(|object| host_buffer(object, self.shared.len()))
            .transpose()?;
        let mut bytes = match &host {
            Some(buffer) => buffer.to_vec(py)?,
            None => self.shared.clone(),
        };
        let widths = self.boundary.widths;
        let layout = ty.layout();
        for item in values.items()?.iter() {
            let (key, value): (String, Bound<'_, PyAny>) = item.extract()?;
            let index = fields.iter().position(|(n, _)| *n == key).ok_or_else(|| {
                let known: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
                PyKeyError::new_err(format!(
                    "the shared block `{name}` has no field `{key}`; its fields are {}",
                    known.join(", ")
                ))
            })?;
            let field_ty = &fields[index].1;
            let offset = widths
                .offset_of(&layout, index)
                .ok_or_else(|| VMError::new_err("a shared field outside its block"))?;
            let end = offset + widths.size_of(&field_ty.layout());
            let slot = bytes
                .get_mut(offset..end)
                .ok_or_else(|| VMError::new_err("a shared field past the end of its block"))?;
            crate::convert::write_flat(slot, &value, field_ty, widths)?;
        }
        match host {
            Some(buffer) => buffer.copy_from_slice(py, &bytes),
            None => {
                self.shared = bytes;
                Ok(())
            }
        }
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
            .find(|(_, slot)| slot.callable.is_none() && !slot.library)
        {
            return Err(VMError::new_err(format!(
                "the native `{name}` is not registered; pass it in `natives=` or call \
                 `vm.register`"
            )));
        }
        Ok(())
    }

    /// Copy a Python buffer into the VM's shared block, returning it for the flush.
    fn sync_shared(&mut self, shared: Option<&Bound<'_, PyAny>>) -> PyResult<Option<PyBuffer<u8>>> {
        let Some(object) = shared else {
            return Ok(None);
        };
        if self.shared.is_empty() {
            return Err(PyTypeError::new_err(
                "the module declares no shared `data` block, so `shared` must be omitted",
            ));
        }
        let buffer = host_buffer(object, self.shared.len())?;
        buffer.copy_to_slice(object.py(), &mut self.shared)?;
        Ok(Some(buffer))
    }

    /// The shared block's declared type, or the reason it cannot be converted.
    fn typed_shared(&self) -> PyResult<&Ty> {
        self.shared_type
            .as_ref()
            .map_err(|why| PyTypeError::new_err(format!("no typed shared access: {why}")))
    }

    /// Copy the VM's shared block back to the Python buffer it was synced from.
    fn flush_shared(&self, py: Python<'_>, host: Option<PyBuffer<u8>>) -> PyResult<()> {
        match host {
            Some(buffer) => buffer.copy_from_slice(py, &self.shared),
            None => Ok(()),
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
        // An exception a native raised but the script then handled, through the
        // native-error construct, belongs to no failure and is dropped.
        let raised = self.native_error.borrow_mut().take();
        self.suspended = matches!(outcome, Ok(VmState::Yielded(_)));
        self.at_reset = matches!(outcome, Ok(VmState::Reset));
        let state = match outcome {
            Ok(state) => state,
            Err(VmError::NotSuspended) => return Err(vm_error(&VmError::NotSuspended)),
            Err(error) => {
                self.failed = true;
                // A native's own exception reaches the caller unchanged. One the
                // script handled earlier in the same run belongs to no failure.
                if let Some(raised) = raised.filter(|_| natives::carries_stored_exception(&error)) {
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
        let resolve = |i: usize| self.vm.resolve_opaque(i);
        let ctx = ReadContext {
            arena: self.vm.arena(),
            widths: self.boundary.widths,
            opaque: &resolve,
        };
        let converted = match &self.boundary.signature {
            Some(signature) => to_py(py, &value, declared(&signature.ret, "the result")?, &ctx)?,
            None => to_py_untyped(py, &value, &ctx)?,
        };
        states.getattr(class)?.call1((converted,))
    }
}
