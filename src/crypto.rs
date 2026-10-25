//! Keys for signed and encrypted modules, the book's chapters 26 and 36.
//!
//! # Trust boundary
//!
//! Signing, encryption, decryption and verification are Keleusma's functions, built on
//! `ed25519-dalek`, `x25519-dalek` and `aes-gcm`. This module only turns Python bytes
//! into those libraries' key types and checks their lengths. It implements no
//! cryptographic primitive.
//!
//! A signing seed or a decryption key passed from Python lives in Python `bytes`, which
//! the binding cannot zero after use. A host that guards such keys should keep them out
//! of long-lived Python objects and logs. Verifying keys are public.

use ed25519_dalek::{SigningKey, VerifyingKey};
use keleusma::encryption::{X25519_PRIVATE_KEY_LEN, public_key_from_private};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// The length of an Ed25519 seed, public key, and X25519 key, in bytes.
const KEY_LEN: usize = 32;

fn exact_key<'a>(bytes: &'a [u8], what: &str) -> PyResult<&'a [u8; KEY_LEN]> {
    bytes.try_into().map_err(|_| {
        PyValueError::new_err(format!(
            "{what} must be exactly {KEY_LEN} bytes, not {}",
            bytes.len()
        ))
    })
}

/// An Ed25519 signing key from its 32-byte seed.
pub fn signing_key(seed: &[u8]) -> PyResult<SigningKey> {
    Ok(SigningKey::from_bytes(exact_key(seed, "a signing seed")?))
}

/// An Ed25519 verifying key from its 32-byte encoding, refused if it is not a valid
/// curve point.
pub fn verifying_key(bytes: &[u8]) -> PyResult<VerifyingKey> {
    VerifyingKey::from_bytes(exact_key(bytes, "a verifying key")?)
        .map_err(|_| PyValueError::new_err("not a valid Ed25519 verifying key"))
}

/// An X25519 key, checked only for its length, as Keleusma's API takes it.
pub fn x25519_key<'a>(bytes: &'a [u8], what: &str) -> PyResult<&'a [u8; X25519_PRIVATE_KEY_LEN]> {
    exact_key(bytes, what)
}

/// The Ed25519 public key for a 32-byte signing seed, the key a VM trusts.
#[pyfunction]
pub fn signing_public_key<'py>(py: Python<'py>, seed: &[u8]) -> PyResult<Bound<'py, PyBytes>> {
    let key = signing_key(seed)?;
    Ok(PyBytes::new(py, key.verifying_key().as_bytes()))
}

/// The X25519 public key for a 32-byte decryption key, the key a module is encrypted to.
#[pyfunction]
pub fn encryption_public_key<'py>(
    py: Python<'py>,
    private_key: &[u8],
) -> PyResult<Bound<'py, PyBytes>> {
    let private = x25519_key(private_key, "a decryption key")?;
    Ok(PyBytes::new(py, &public_key_from_private(private)))
}
