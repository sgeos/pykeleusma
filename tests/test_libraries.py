"""Keleusma's bundled native libraries, the book's chapter 33.

A host offers a library by naming it in `libraries=`. The script still declares each
native it uses, which `keleusma.library_signatures` supplies. The shell library is
exercised only through reading the environment, which has no side effect.
"""

import math
import os
from typing import cast

import pytest

import keleusma

SQRT = keleusma.compile(
    "use math::sqrt(Float) -> Float\nfn main(x: Float) -> Float { math::sqrt(x) }"
)


def test_a_library_native_runs_when_the_host_offers_the_library() -> None:
    assert keleusma.VM(SQRT, libraries=["math"]).call(9.0) == keleusma.Finished(3.0)


def test_a_library_is_not_offered_unless_named() -> None:
    vm = keleusma.VM(SQRT)
    with pytest.raises(keleusma.VMError, match="not registered"):
        vm.call(9.0)


def test_naming_a_library_twice_is_harmless() -> None:
    vm = keleusma.VM(SQRT, libraries=["math", "math"])
    assert vm.call(4.0) == keleusma.Finished(2.0)


def test_an_unknown_library_is_refused() -> None:
    with pytest.raises(ValueError, match="unknown library `nope`"):
        keleusma.VM(SQRT, libraries=["nope"])
    with pytest.raises(ValueError, match="unknown library"):
        keleusma.library_signatures("nope")


def _negate(x: float) -> float:
    return -x


def _plus_one(x: float) -> float:
    return x + 1


def test_a_host_callable_overrides_a_library_native() -> None:
    vm = keleusma.VM(SQRT, libraries=["math"], natives={"math::sqrt": _negate})
    assert vm.call(9.0) == keleusma.Finished(-9.0)
    vm.register("math::sqrt", _plus_one)
    assert vm.call(9.0) == keleusma.Finished(10.0)


def test_a_composite_library_result_converts() -> None:
    module = keleusma.compile(
        "use audio::pan_law(Float) -> (Float, Float)\n"
        "fn main(x: Float) -> (Float, Float) { audio::pan_law(x) }"
    )
    state = keleusma.VM(module, libraries=["audio"]).call(0.0)
    assert isinstance(state, keleusma.Finished)
    value: object = state.value
    left, right = cast(tuple[float, float], value)
    # An equal-power pan at the centre gives each side the square root of a half.
    assert left == pytest.approx(math.sqrt(0.5))
    assert right == pytest.approx(math.sqrt(0.5))


def test_the_shell_library_reads_the_environment(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    name = "PYKELEUSMA_TEST_VARIABLE"
    monkeypatch.setenv(name, "present")
    monkeypatch.delenv(name + "_UNSET", raising=False)
    module = keleusma.compile(
        "use shell::getenv(Text) -> Option<Text>\n"
        "fn main(n: Text) -> Option<Text> { shell::getenv(n) }"
    )
    vm = keleusma.VM(module, libraries=["shell"])
    assert vm.call(name) == keleusma.Finished(os.environ[name])
    assert vm.call(name + "_UNSET") == keleusma.Finished(None)


@pytest.mark.parametrize("library", ["math", "audio", "shell"])
def test_the_published_signatures_compile(library: str) -> None:
    # Every declaration Keleusma publishes compiles, so a script can include the
    # whole block to have its calls type-checked.
    signatures = keleusma.library_signatures(library)
    assert signatures.count("use ") > 0
    keleusma.compile(signatures + "\nfn main() -> Word { 0 }")
