"""`Multiword<N, F>` big numbers at the boundary, the book's chapter 23.

A multiword holds a signed two's complement integer in `N` words, low word first, and
its value is that integer divided by `2**F`. It converts to an `int` when `F` is zero
and to an exact `Fraction` otherwise. Expected values are computed in Python from the
same definition rather than pinned.
"""

from fractions import Fraction

import pytest

import keleusma


def _call(source: str, *args: object) -> object:
    state = keleusma.VM(keleusma.compile(source)).call(*args)
    assert isinstance(state, (keleusma.Finished, keleusma.Yielded))
    return state.value


ADD = "fn main(a: Multiword<2>, b: Multiword<2>) -> Multiword<2> { a + b }"


@pytest.mark.parametrize(
    ("a", "b"),
    [(2**64 - 1, 1), (-5, 3), (2**100, 2**100), (-(2**126), -(2**126))],
)
def test_addition_carries_across_words(a: int, b: int) -> None:
    assert _call(ADD, a, b) == a + b


def test_the_words_are_low_first() -> None:
    value = 7 * 2**64 + 3
    assert _call("fn main(a: Multiword<2>) -> Word { a[0] }", value) == 3
    assert _call("fn main(a: Multiword<2>) -> Word { a[1] }", value) == 7


def test_a_script_built_multiword_converts_out() -> None:
    source = (
        "fn main(x: Word) -> (Multiword<3>, Word) { ((x, 0, 1) as Multiword<3>, x) }"
    )
    assert _call(source, 4) == (2**128 + 4, 4)


def test_a_fractional_multiword_is_an_exact_fraction() -> None:
    source = (
        "fn main(a: Multiword<2, 1>, b: Multiword<2, 1>) -> Multiword<2, 1> { a * b }"
    )
    result = _call(source, Fraction(3, 2), 2)
    assert result == Fraction(3, 2) * 2
    assert isinstance(result, Fraction)


def test_a_stream_yields_a_multiword() -> None:
    source = "loop main(a: Multiword<2>) -> Multiword<2> { let n = yield a + a; n }"
    assert _call(source, 2**100) == 2**101


@pytest.mark.parametrize(
    ("value", "error"),
    [
        (2**127, OverflowError),
        (-(2**127) - 1, OverflowError),
        (True, TypeError),
        ("1", TypeError),
    ],
)
def test_a_value_out_of_range_or_of_the_wrong_type_is_refused(
    value: object, error: type[Exception]
) -> None:
    vm = keleusma.VM(keleusma.compile("fn main(a: Multiword<2>) -> Word { 0 }"))
    with pytest.raises(error):
        vm.call(value)
    assert not vm.failed


def test_a_value_without_an_exact_scaled_form_is_refused() -> None:
    vm = keleusma.VM(keleusma.compile("fn main(a: Multiword<2, 1>) -> Word { 0 }"))
    with pytest.raises(ValueError, match="not exactly representable"):
        vm.call(0.25)


def test_the_extremes_round_trip() -> None:
    echo = "fn main(a: Multiword<2>) -> Multiword<2> { a }"
    assert _call(echo, 2**127 - 1) == 2**127 - 1
    assert _call(echo, -(2**127)) == -(2**127)
