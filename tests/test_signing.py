"""Signed and encrypted modules, the book's chapters 26 and 36.

Keys are fixed byte strings, not secrets, so the tests are deterministic apart from the
ephemeral key of each encryption, which comes from the operating system. Signing,
verification, encryption and decryption are Keleusma's own, and these tests check that
the binding admits what Keleusma admits and refuses what it refuses.
"""

import pytest

import keleusma

SEED = bytes(range(32))
OTHER_SEED = bytes(32)
PUBLIC = keleusma.signing_public_key(SEED)
OTHER_PUBLIC = keleusma.signing_public_key(OTHER_SEED)
DECRYPTION_KEY = bytes(range(32, 64))
RECIPIENT = keleusma.encryption_public_key(DECRYPTION_KEY)

SIGNED = keleusma.compile("signed fn main() -> Word { 42 }")


def test_a_signed_module_loads_and_runs_with_its_key() -> None:
    module = keleusma.load_module(SIGNED.to_signed_bytes(SEED), verifying_keys=[PUBLIC])
    assert module.is_signed
    assert keleusma.VM(module).call() == keleusma.Finished(42)


def test_any_one_of_several_keys_may_verify() -> None:
    data = SIGNED.to_signed_bytes(SEED)
    module = keleusma.load_module(data, verifying_keys=[OTHER_PUBLIC, PUBLIC])
    assert keleusma.VM(module).call() == keleusma.Finished(42)


def test_a_signed_module_without_keys_is_refused() -> None:
    with pytest.raises(keleusma.VerifyError, match="verifying_keys"):
        keleusma.load_module(SIGNED.to_signed_bytes(SEED))


def test_a_signed_module_with_the_wrong_key_is_refused() -> None:
    with pytest.raises(keleusma.VerifyError):
        keleusma.load_module(
            SIGNED.to_signed_bytes(SEED), verifying_keys=[OTHER_PUBLIC]
        )


def test_a_tampered_signed_module_is_refused() -> None:
    data = bytearray(SIGNED.to_signed_bytes(SEED))
    data[len(data) // 2] ^= 1
    with pytest.raises((keleusma.VerifyError, keleusma.LoadError)):
        keleusma.load_module(bytes(data), verifying_keys=[PUBLIC])


def test_keys_for_an_unsigned_module_are_refused() -> None:
    # A host that expects signed code cannot be handed unsigned code instead.
    plain = keleusma.compile("fn main() -> Word { 1 }").to_bytes()
    with pytest.raises(keleusma.VerifyError, match="not signed"):
        keleusma.load_module(plain, verifying_keys=[PUBLIC])


def test_a_module_declared_signed_runs_only_from_signed_bytes() -> None:
    with pytest.raises(keleusma.VerifyError, match="declared `signed`"):
        keleusma.VM(SIGNED)


def test_an_encrypted_module_loads_with_both_keys() -> None:
    data = SIGNED.to_signed_bytes(SEED, recipient_key=RECIPIENT)
    module = keleusma.load_module(
        data, verifying_keys=[PUBLIC], decryption_key=DECRYPTION_KEY
    )
    assert module.is_signed
    assert keleusma.VM(module).call() == keleusma.Finished(42)


def test_each_encryption_uses_a_fresh_ephemeral_key() -> None:
    first = SIGNED.to_signed_bytes(SEED, recipient_key=RECIPIENT)
    second = SIGNED.to_signed_bytes(SEED, recipient_key=RECIPIENT)
    assert first != second


def test_an_encrypted_module_needs_both_keys() -> None:
    data = SIGNED.to_signed_bytes(SEED, recipient_key=RECIPIENT)
    with pytest.raises(keleusma.VerifyError, match="decryption_key"):
        keleusma.load_module(data, verifying_keys=[PUBLIC])
    with pytest.raises(keleusma.VerifyError, match="verifying_keys"):
        keleusma.load_module(data, decryption_key=DECRYPTION_KEY)


def test_an_encrypted_module_with_a_wrong_key_is_refused() -> None:
    data = SIGNED.to_signed_bytes(SEED, recipient_key=RECIPIENT)
    with pytest.raises(keleusma.VerifyError):
        keleusma.load_module(data, verifying_keys=[PUBLIC], decryption_key=bytes(32))
    with pytest.raises(keleusma.VerifyError):
        keleusma.load_module(
            data, verifying_keys=[OTHER_PUBLIC], decryption_key=DECRYPTION_KEY
        )


def test_a_decryption_key_for_a_plaintext_module_is_refused() -> None:
    with pytest.raises(keleusma.VerifyError, match="not encrypted"):
        keleusma.load_module(
            SIGNED.to_signed_bytes(SEED),
            verifying_keys=[PUBLIC],
            decryption_key=DECRYPTION_KEY,
        )


@pytest.mark.parametrize(
    ("call", "message"),
    [
        (lambda: keleusma.signing_public_key(b"short"), "a signing seed"),
        (lambda: keleusma.encryption_public_key(b"short"), "a decryption key"),
        (lambda: SIGNED.to_signed_bytes(b"short"), "a signing seed"),
        (
            lambda: SIGNED.to_signed_bytes(SEED, recipient_key=b"short"),
            "a recipient key",
        ),
        (
            lambda: keleusma.load_module(b"", verifying_keys=[b"short"]),
            "a verifying key",
        ),
    ],
)
def test_a_key_of_the_wrong_length_is_refused(call: object, message: str) -> None:
    with pytest.raises(ValueError, match=f"{message} must be exactly 32 bytes"):
        call()  # pyright: ignore[reportCallIssue]


def test_a_verifying_key_that_is_not_a_curve_point_is_refused() -> None:
    with pytest.raises(ValueError, match="not a valid Ed25519"):
        keleusma.load_module(b"", verifying_keys=[bytes([2]) + bytes(31)])
