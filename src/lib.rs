//! Python bindings for the Keleusma scripting language.
//!
//! The binding is planned in `docs/architecture/BINDING_DESIGN.md`, and the first
//! milestone in `docs/roadmap/V0_1_0.md`. Task 1, owning the arena and the VM together,
//! lives in [`owned_vm`], task 2, compilation, in [`compile`], task 3, the Python `VM`
//! class, in [`vm`], and task 4, value conversion by declared type, in [`convert`] and
//! [`types`].

mod compile;
mod convert;
mod owned_vm;
mod types;
mod vm;
mod yield_check;

use pyo3::prelude::*;

/// The Keleusma release this build is pinned to in `Cargo.toml`.
///
/// `tests/test_smoke.py` reads the pin from `Cargo.toml` and compares, so the two
/// cannot drift silently.
const KELEUSMA_VERSION: &str = "0.2.2";

/// The default arena budget for composite arguments, in bytes.
///
/// Measured on Keleusma 0.2.2, a tuple argument costs about twice its text length
/// plus a few tens of bytes, and accumulates across the yields of one stream
/// iteration. 4096 bytes therefore covers events of roughly a kilobyte of text in a
/// stream with one yield per iteration. A host with larger events passes its own
/// figure to `keleusma.VM`.
pub const DEFAULT_ARGUMENT_BYTES: usize = 4096;

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
    module.add("DEFAULT_ARGUMENT_BYTES", DEFAULT_ARGUMENT_BYTES)?;
    module.add_class::<compile::Module>()?;
    module.add_function(wrap_pyfunction!(compile::compile, module)?)?;
    module.add_class::<vm::VM>()?;
    module.add_function(wrap_pyfunction!(_live_vm_count, module)?)?;
    Ok(())
}
