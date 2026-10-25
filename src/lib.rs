//! Python bindings for the Keleusma scripting language.
//!
//! The binding is planned in `docs/architecture/BINDING_DESIGN.md`, and the first
//! milestone in `docs/roadmap/V0_1_0.md`. Task 1 of that milestone, owning the arena
//! and the VM together, lives in [`owned_vm`]. The `_OwnedVmProbe` class below exists
//! only to exercise that ownership from Python, and is removed when `keleusma.VM`
//! replaces it in task 3.

mod owned_vm;

use keleusma::bytecode::{Module, Value};
use keleusma::compiler::compile;
use keleusma::lexer::tokenize;
use keleusma::parser::parse;
use keleusma::vm::VmState;
use pyo3::exceptions::{PyMemoryError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;

use crate::owned_vm::{OwnedVm, OwnedVmError};

/// The Keleusma release this build is pinned to in `Cargo.toml`.
///
/// `tests/test_smoke.py` reads the pin from `Cargo.toml` and compares, so the two
/// cannot drift silently.
const KELEUSMA_VERSION: &str = "0.2.2";

/// Compile source text into a module.
///
/// Temporary. Task 2 replaces this with `keleusma.compile` and dedicated exception
/// classes that carry the line and column. Until then each stage reports a
/// `ValueError` whose message names the stage and the location.
fn compile_source(source: &str) -> PyResult<Module> {
    let tokens = tokenize(source).map_err(|e| {
        PyValueError::new_err(format!(
            "lex error at {}:{}: {}",
            e.span.line, e.span.column, e.message
        ))
    })?;
    let program = parse(&tokens).map_err(|e| {
        PyValueError::new_err(format!(
            "parse error at {}:{}: {}",
            e.span.line, e.span.column, e.message
        ))
    })?;
    compile(&program).map_err(|e| {
        PyValueError::new_err(format!(
            "compile error at {}:{}: {}",
            e.span.line, e.span.column, e.message
        ))
    })
}

fn owned_vm_error(error: OwnedVmError) -> PyErr {
    match error {
        OwnedVmError::ArenaAllocation { .. } => PyMemoryError::new_err(error.to_string()),
        OwnedVmError::Vm(_) => PyValueError::new_err(error.to_string()),
        OwnedVmError::PersistentRegion { .. } | OwnedVmError::CapacityOverflow => {
            PyRuntimeError::new_err(error.to_string())
        }
    }
}

/// Convert the scalar results the probe needs. Task 4 replaces this with full
/// conversion, so anything else is refused rather than guessed at.
fn probe_value(value: &Value) -> PyResult<Option<i64>> {
    match value {
        Value::Unit => Ok(None),
        Value::Int(n) => Ok(Some(*n)),
        other => Err(PyTypeError::new_err(format!(
            "value conversion is not implemented yet for {other:?}"
        ))),
    }
}

/// A Keleusma VM owned together with its arena, for testing that ownership.
///
/// Temporary and private. `unsendable` because the VM is neither `Send` nor `Sync`.
/// PyO3 raises if another thread touches the object, and drops it on the creating
/// thread.
#[pyclass(unsendable, module = "keleusma._keleusma", name = "_OwnedVmProbe")]
struct OwnedVmProbe {
    vm: OwnedVm,
}

#[pymethods]
impl OwnedVmProbe {
    #[new]
    fn new(source: &str) -> PyResult<Self> {
        let module = compile_source(source)?;
        let vm = OwnedVm::new(module).map_err(owned_vm_error)?;
        Ok(Self { vm })
    }

    /// The arena's total capacity in bytes.
    #[getter]
    fn arena_capacity(&self) -> usize {
        self.vm.arena_capacity()
    }

    /// Call the entry point with no arguments, returning the state name and an
    /// integer or `None` for a unit value.
    fn call(&mut self) -> PyResult<(&'static str, Option<i64>)> {
        let state = self
            .vm
            .call_no_args()
            .map_err(|e| PyRuntimeError::new_err(format!("{e:?}")))?;
        match state {
            VmState::Yielded(value) => Ok(("yielded", probe_value(&value)?)),
            VmState::Finished(value) => Ok(("finished", probe_value(&value)?)),
            VmState::Reset => Ok(("reset", None)),
            VmState::BreakpointHit { .. } => Err(PyRuntimeError::new_err(
                "a breakpoint was hit, but the binding arms none",
            )),
        }
    }
}

/// The number of VMs alive in this process, for leak tests.
#[pyfunction]
fn _live_vm_count() -> usize {
    owned_vm::live_count()
}

#[pymodule]
fn _keleusma(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("KELEUSMA_VERSION", KELEUSMA_VERSION)?;
    // Read from the linked crate rather than restated, so the value describes the
    // runtime that is actually in the wheel.
    module.add("BYTECODE_VERSION", keleusma::bytecode::BYTECODE_VERSION)?;
    module.add_class::<OwnedVmProbe>()?;
    module.add_function(wrap_pyfunction!(_live_vm_count, module)?)?;
    Ok(())
}
