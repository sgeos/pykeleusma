"""Replacing a VM's module at an iteration boundary, the book's chapters 26 and 37.

The arena is sized when the VM is built, so a VM is built from the module with the
larger footprint and the smaller one swapped in. Signed swaps check the VM's trust
matrix, which the binding enforces itself because Keleusma 0.2.2 does not, as
`src/owned_vm.rs` explains.
"""

import pytest

import keleusma

TIMES_TEN = keleusma.compile("loop main(t: Word) -> Word { let n = yield t * 10; n }")
ECHO = keleusma.compile("loop main(t: Word) -> Word { let n = yield t; n }")
SMALL = keleusma.compile("fn main(t: Word) -> Word { t }")
SEED = bytes(range(32))
PUBLIC = keleusma.signing_public_key(SEED)


def _signed(source: str) -> keleusma.Module:
    data = keleusma.compile(source).to_signed_bytes(SEED)
    return keleusma.load_module(data, verifying_keys=[PUBLIC])


def test_a_swap_at_a_reset_runs_the_new_module() -> None:
    vm = keleusma.VM(TIMES_TEN)
    assert vm.call(1) == keleusma.Yielded(10)
    assert vm.resume(0) == keleusma.Reset()
    vm.replace_module(ECHO)
    assert vm.call(2) == keleusma.Yielded(2)


def test_a_swap_while_suspended_is_refused() -> None:
    vm = keleusma.VM(TIMES_TEN)
    vm.call(1)
    with pytest.raises(keleusma.VMError, match="iteration boundary"):
        vm.replace_module(ECHO)
    assert vm.resume(0) == keleusma.Reset()


def test_a_module_larger_than_the_arena_is_refused_and_the_old_one_kept() -> None:
    big = keleusma.compile(
        "fn main(t: Word) -> Word "
        "{ let a = [t, t, t, t, t, t, t, t, t, t, t, t, t, t, t, t]; a[0] }"
    )
    vm = keleusma.VM(SMALL)
    with pytest.raises(keleusma.VerifyError, match="larger footprint"):
        vm.replace_module(big)
    assert not vm.failed
    assert vm.call(7) == keleusma.Finished(7)


def test_shared_data_survives_a_swap_with_the_same_layout() -> None:
    double = keleusma.compile(
        "data st { n: Word }\n"
        "loop main(t: Word) -> Word { st.n = st.n + 2 * t; let n = yield st.n; n }"
    )
    single = keleusma.compile(
        "data st { n: Word }\n"
        "loop main(t: Word) -> Word { st.n = st.n + t; let n = yield st.n; n }"
    )
    vm = keleusma.VM(double)
    assert vm.call(5) == keleusma.Yielded(10)
    vm.resume(0)
    vm.replace_module(single)
    assert vm.call(1) == keleusma.Yielded(11)


def test_a_swap_that_changes_the_data_layout_is_refused() -> None:
    one = keleusma.compile(
        "data st { n: Word }\nfn main(t: Word) -> Word { st.n = t; st.n }"
    )
    two = keleusma.compile(
        "data st { n: Word, m: Word }\nfn main(t: Word) -> Word { st.n = t; st.n }"
    )
    vm = keleusma.VM(two)
    with pytest.raises(keleusma.VerifyError, match="build a new VM"):
        vm.replace_module(one)


def test_private_data_starts_afresh_after_a_swap() -> None:
    module = keleusma.compile(
        "private data st { n: Word }\nfn main(t: Word) -> Word { st.n = t; st.n }"
    )
    reader = keleusma.compile(
        "private data st { n: Word }\n"
        "fn main(t: Word) -> Word { let old = st.n + 0; st.n = t; old }"
    )
    vm = keleusma.VM(reader)  # The larger footprint of the two.
    vm.replace_module(module)
    assert vm.call(5) == keleusma.Finished(5)
    vm.replace_module(reader)
    # A fresh private slot holds `()`, so reading it before writing fails.
    with pytest.raises(keleusma.VMError, match="Unit"):
        vm.call(0)


def _identity(x: int) -> int:
    return x


def _plus_100(x: int) -> int:
    return x + 100


def test_a_native_new_to_the_swapped_module_must_be_registered() -> None:
    uses_g = keleusma.compile(
        "use host::g(Word) -> Word\nfn main(t: Word) -> Word { host::g(t) }"
    )
    vm = keleusma.VM(uses_g, natives={"host::g": _identity})
    vm.replace_module(SMALL)
    uses_h = keleusma.compile(
        "use host::h(Word) -> Word\nfn main(t: Word) -> Word { host::h(t) }"
    )
    vm.replace_module(uses_h)
    with pytest.raises(keleusma.VMError, match="not registered"):
        vm.call(1)
    vm.register("host::h", _plus_100)
    assert vm.call(1) == keleusma.Finished(101)


def test_a_swap_recovers_a_failed_vm() -> None:
    vm = keleusma.VM(keleusma.compile("fn main(t: Word) -> Word { 10 / t }"))
    with pytest.raises(keleusma.VMError):
        vm.call(0)
    vm.replace_module(SMALL)
    assert not vm.failed
    assert vm.call(3) == keleusma.Finished(3)


def test_a_module_declared_signed_cannot_be_swapped_in_unsigned() -> None:
    with pytest.raises(keleusma.VerifyError, match="declared `signed`"):
        keleusma.VM(SMALL).replace_module(
            keleusma.compile("signed fn main() -> Word { 1 }")
        )


# The trust matrix.


def test_a_signed_swap_needs_a_trusted_key() -> None:
    update = _signed("signed loop main(t: Word) -> Word { let n = yield t + 1; n }")
    vm = keleusma.VM(TIMES_TEN)
    with pytest.raises(keleusma.LoadError, match="did not verify"):
        vm.replace_module(update)
    vm.register_verifying_key(keleusma.signing_public_key(bytes(32)))
    with pytest.raises(keleusma.LoadError, match="did not verify"):
        vm.replace_module(update)
    vm.register_verifying_key(PUBLIC)
    vm.replace_module(update)
    assert vm.call(1) == keleusma.Yielded(2)


def test_once_a_key_is_trusted_unsigned_swaps_are_refused() -> None:
    vm = keleusma.VM(TIMES_TEN)
    vm.register_verifying_key(PUBLIC)
    with pytest.raises(keleusma.LoadError):
        vm.replace_module(ECHO)
    assert vm.call(1) == keleusma.Yielded(10)


def test_a_vm_built_from_signed_code_trusts_its_keys() -> None:
    first = _signed("signed loop main(t: Word) -> Word { let n = yield t * 10; n }")
    second = _signed("signed loop main(t: Word) -> Word { let n = yield t; n }")
    vm = keleusma.VM(first)
    with pytest.raises(keleusma.LoadError):
        vm.replace_module(ECHO)
    vm.replace_module(second)
    assert vm.call(4) == keleusma.Yielded(4)


def test_an_encrypted_module_swaps_in_when_its_signer_is_trusted() -> None:
    # The signature covers the encrypted bytes, which the module keeps, so the VM's
    # trust matrix checks it at the swap without the decryption key.
    decryption_key = bytes(range(32, 64))
    data = keleusma.compile(
        "signed loop main(t: Word) -> Word { let n = yield t + 1; n }"
    ).to_signed_bytes(
        SEED, recipient_key=keleusma.encryption_public_key(decryption_key)
    )
    module = keleusma.load_module(
        data, verifying_keys=[PUBLIC], decryption_key=decryption_key
    )
    vm = keleusma.VM(TIMES_TEN)
    with pytest.raises(keleusma.LoadError, match="did not verify"):
        vm.replace_module(module)
    vm.register_verifying_key(PUBLIC)
    vm.replace_module(module)
    assert vm.call(1) == keleusma.Yielded(2)


def test_a_malformed_verifying_key_is_refused() -> None:
    with pytest.raises(ValueError, match="32 bytes"):
        keleusma.VM(SMALL).register_verifying_key(b"short")


def _four(x: int) -> int:
    return 4


def test_a_native_converts_by_the_swapped_modules_declaration() -> None:
    # The same callable serves both modules. Before the fix, its closure kept the
    # first module's declaration and built a Word where the second expects a Float.
    as_word = keleusma.compile(
        "use host::f(Word) -> Word\nfn main(x: Word) -> Word { host::f(x) }"
    )
    as_float = keleusma.compile(
        "use host::f(Word) -> Float\nfn main(x: Word) -> Float { host::f(x) }"
    )
    vm = keleusma.VM(as_word, natives={"host::f": _four})
    assert vm.call(1) == keleusma.Finished(4)
    vm.replace_module(as_float)
    state = vm.call(1)
    assert state == keleusma.Finished(4.0)
    assert isinstance(state, keleusma.Finished) and isinstance(state.value, float)
