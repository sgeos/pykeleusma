//! Python bindings for the Keleusma scripting language.
//!
//! This is the seed. It exposes version information only, which proves that the
//! pinned `keleusma` release and PyO3 build and link together. The binding is
//! planned in `docs/architecture/BINDING_DESIGN.md`, and the first milestone in
//! `docs/roadmap/V0_1_0.md`.

use pyo3::prelude::*;

/// The Keleusma release this build is pinned to in `Cargo.toml`.
///
/// `tests/test_smoke.py` reads the pin from `Cargo.toml` and compares, so the two
/// cannot drift silently.
const KELEUSMA_VERSION: &str = "0.2.2";

#[pymodule]
fn _keleusma(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("KELEUSMA_VERSION", KELEUSMA_VERSION)?;
    // Read from the linked crate rather than restated, so the value describes the
    // runtime that is actually in the wheel.
    module.add("BYTECODE_VERSION", keleusma::bytecode::BYTECODE_VERSION)?;
    Ok(())
}
