//! Python bindings for the Keleusma scripting language.
//!
//! The binding is planned in `docs/architecture/BINDING_DESIGN.md`, and the first
//! milestone in `docs/roadmap/V0_1_0.md`. Task 1, owning the arena and the VM together,
//! lives in [`owned_vm`], task 2, compilation, in [`compile`], task 3, the Python `VM`
//! class, in [`vm`], and task 4, value conversion by declared type, in [`convert`] and
//! [`types`], and task 7, Python natives, in [`natives`].

mod canaries;
mod compat;
mod compile;
mod convert;
mod crypto;
mod external;
mod libraries;
mod natives;
mod opaque;
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

/// The `use` declarations of one of Keleusma's bundled libraries, `"math"`, `"audio"`
/// or `"shell"`, which a script may include so its calls are type-checked.
#[pyfunction]
fn library_signatures(name: &str) -> PyResult<&'static str> {
    libraries::Library::named(name)
        .map(libraries::Library::signatures)
        .ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "unknown library `{name}`; the libraries are math, audio and shell"
            ))
        })
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
    module.add("DEFAULT_ARGUMENT_BYTES", DEFAULT_ARGUMENT_BYTES)?;
    module.add(
        "OP_NAMES",
        pyo3::types::PyTuple::new(module.py(), compat::OP_NAMES)?,
    )?;
    module.add_class::<compile::Module>()?;
    module.add_function(wrap_pyfunction!(compile::compile, module)?)?;
    module.add_function(wrap_pyfunction!(compile::load_module, module)?)?;
    module.add_class::<vm::VM>()?;
    module.add_function(wrap_pyfunction!(_live_vm_count, module)?)?;
    module.add_function(wrap_pyfunction!(library_signatures, module)?)?;
    module.add_function(wrap_pyfunction!(crypto::signing_public_key, module)?)?;
    module.add_function(wrap_pyfunction!(crypto::encryption_public_key, module)?)?;
    Ok(())
}
