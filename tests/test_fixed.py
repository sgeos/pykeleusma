"""`keleusma.Fixed`, Keleusma's fixed-point value as an exact Python number.

It subclasses `fractions.Fraction`, so these tests compare against `Fraction` of the
same value rather than against hand-computed constants.
"""

import copy
import math
import pickle
from decimal import Decimal
from fractions import Fraction

import pytest

from keleusma import Fixed


def test_the_value_is_raw_over_two_to_the_frac_bits() -> None:
    value = Fixed(98304, 16)
    assert (value.raw, value.frac_bits) == (98304, 16)
    assert value == Fraction(98304, 2**16) == Fraction(3, 2)


def test_it_behaves_as_the_same_fraction() -> None:
    value = Fixed(-5, 2)
    same = Fraction(-5, 4)
    assert isinstance(value, Fraction)
    assert hash(value) == hash(same)
    assert value < 0 < Fixed(1, 0)
    assert float(value) == float(same)
    assert int(value) == int(same)
    assert round(value) == round(same)
    assert math.floor(value) == math.floor(same)
    assert value.as_fraction() == same
    assert type(value.as_fraction()) is Fraction


def test_arithmetic_returns_a_plain_fraction() -> None:
    # Fixed-width overflow and rounding are Keleusma's semantics, so Python
    # arithmetic leaves the width behind rather than imitating them.
    total = Fixed(3, 2) + Fixed(1, 2)
    assert total == 1
    assert type(total) is Fraction


@pytest.mark.parametrize(
    ("value", "bits"),
    [
        (2, 0),
        (-2.75, 2),
        (Fraction(1, 1024), 10),
        (Decimal("0.5"), 1),
        (Fixed(3, 1), 4),
    ],
)
def test_from_value_converts_exact_numbers(value: object, bits: int) -> None:
    result = Fixed.from_value(value, bits)
    assert result.frac_bits == bits
    assert isinstance(value, int | float | Fraction | Decimal)
    assert result == Fraction(value)


@pytest.mark.parametrize(
    ("value", "bits", "error"),
    [
        (0.1, 16, ValueError),
        (Fraction(1, 3), 62, ValueError),
        (float("inf"), 4, ValueError),
        (float("nan"), 4, ValueError),
        (Decimal("NaN"), 4, ValueError),
        (2**62, 2, OverflowError),
        (True, 4, TypeError),
        ("1", 4, TypeError),
        (1, 63, ValueError),
        (1, -1, ValueError),
        (1, 4.0, TypeError),
    ],
)
def test_from_value_refuses_rather_than_rounds(
    value: object, bits: object, error: type[Exception]
) -> None:
    with pytest.raises(error):
        Fixed.from_value(value, bits)  # pyright: ignore[reportArgumentType]


@pytest.mark.parametrize(
    ("raw", "bits", "error"),
    [
        (2**63, 0, OverflowError),
        (-(2**63) - 1, 0, OverflowError),
        (1.0, 0, TypeError),
        (True, 0, TypeError),
        (1, 63, ValueError),
    ],
)
def test_the_constructor_checks_its_arguments(
    raw: object, bits: int, error: type[Exception]
) -> None:
    with pytest.raises(error):
        Fixed(raw, bits)  # pyright: ignore[reportArgumentType]


def test_it_pickles_and_copies_with_its_width() -> None:
    value = Fixed(7, 3)
    # The round trip uses data this test made, which is what pickle requires.
    restored = pickle.loads(pickle.dumps(value))  # noqa: S301
    assert type(restored) is Fixed
    assert (restored.raw, restored.frac_bits) == (7, 3)
    assert copy.copy(value) is value
    assert copy.deepcopy(value) is value


def test_the_repr_shows_the_representation() -> None:
    assert repr(Fixed(7, 3)) == "Fixed(raw=7, frac_bits=3)"
