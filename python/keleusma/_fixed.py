"""Keleusma's signed fixed-point values, as an exact Python number.

A Keleusma `Fixed<N>` is a 64-bit integer `raw` read as `raw / 2**N`. Its runtime value
carries only `raw`, and `N` comes from the script's declared type, so the binding
supplies it and the Python value keeps it.

`Fixed` subclasses `fractions.Fraction`. Equality, hashing, ordering, arithmetic,
`float()`, `int()`, `round()` and `math.floor()` therefore behave exactly as they do
for the same rational value, which is the standard library's convention for exact
numbers. Arithmetic returns a plain `Fraction`, not a `Fixed`, because overflow and
rounding at a fixed width are Keleusma's semantics, which Python should not imitate.
Pass the result back through `Fixed.from_value` to choose a width explicitly.
"""

import math
from decimal import Decimal
from fractions import Fraction
from numbers import Rational
from typing import Self, final

#: The fraction-bit counts Keleusma's parser accepts for `Fixed<N>`.
MIN_FRAC_BITS = 0
MAX_FRAC_BITS = 62

#: The range of the 64-bit integer that holds a fixed-point value.
RAW_MIN = -(2**63)
RAW_MAX = 2**63 - 1


def _check_frac_bits(frac_bits: object) -> int:
    if not isinstance(frac_bits, int) or isinstance(frac_bits, bool):
        raise TypeError(f"frac_bits must be an int, not {type(frac_bits).__name__}")
    if not MIN_FRAC_BITS <= frac_bits <= MAX_FRAC_BITS:
        raise ValueError(
            f"frac_bits must be in the range [{MIN_FRAC_BITS}, {MAX_FRAC_BITS}],"
            f" not {frac_bits}"
        )
    return frac_bits


@final
class Fixed(Fraction):
    """An exact Keleusma `Fixed<N>` value, `raw / 2**frac_bits`."""

    __slots__ = ("_frac_bits", "_raw")

    _raw: int
    _frac_bits: int

    def __new__(cls, raw: int, frac_bits: int) -> Self:
        """Build the value `raw / 2**frac_bits` from Keleusma's representation.

        Raises `TypeError` for a non-integer argument, `ValueError` for a
        fraction-bit count Keleusma does not accept, and `OverflowError` for a raw
        value outside the signed 64-bit range.
        """
        # The annotation does not bind callers that skip type checking, so the type
        # is checked at runtime as well.
        if not isinstance(raw, int) or isinstance(raw, bool):  # pyright: ignore[reportUnnecessaryIsInstance]
            raise TypeError(f"raw must be an int, not {type(raw).__name__}")
        bits = _check_frac_bits(frac_bits)
        if not RAW_MIN <= raw <= RAW_MAX:
            raise OverflowError(f"raw {raw} does not fit a signed 64-bit integer")
        self = super().__new__(cls, raw, 1 << bits)
        self._raw = raw
        self._frac_bits = bits
        return self

    @classmethod
    def from_value(cls, value: object, frac_bits: int) -> "Fixed":
        """Convert a number to `Fixed<frac_bits>` exactly, or raise.

        Accepts `int`, `float`, `Fraction`, `Decimal`, another `Fixed`, and any other
        `numbers.Rational`. Raises `ValueError` if the value has no exact
        representation with `frac_bits` fraction bits, and `OverflowError` if the
        representation does not fit 64 bits. Nothing is rounded, because a silent
        rounding would break the round trip the binding guarantees.
        """
        bits = _check_frac_bits(frac_bits)
        if isinstance(value, bool):
            raise TypeError("a bool is not a fixed-point value")
        if isinstance(value, float):
            if not math.isfinite(value):
                raise ValueError(f"{value!r} has no fixed-point representation")
            exact = Fraction(value)
        elif isinstance(value, Decimal):
            if not value.is_finite():
                raise ValueError(f"{value!r} has no fixed-point representation")
            exact = Fraction(value)
        elif isinstance(value, int | Rational):
            exact = Fraction(value)
        else:
            raise TypeError(
                f"cannot convert {type(value).__name__} to a fixed-point value"
            )
        scaled = exact * (1 << bits)
        if scaled.denominator != 1:
            raise ValueError(
                f"{value!r} is not exactly representable with {bits} fraction bits"
            )
        return cls(scaled.numerator, bits)

    @property
    def raw(self) -> int:
        """The signed 64-bit integer Keleusma stores."""
        return self._raw

    @property
    def frac_bits(self) -> int:
        """The number of fraction bits, the `N` of `Fixed<N>`."""
        return self._frac_bits

    def as_fraction(self) -> Fraction:
        """The same value as a plain `Fraction`, without the width."""
        return Fraction(self.numerator, self.denominator)

    def __repr__(self) -> str:
        return f"Fixed(raw={self._raw}, frac_bits={self._frac_bits})"

    def __reduce__(self) -> tuple[type["Fixed"], tuple[int, int]]:
        # Fraction pickles through its numerator and denominator, which would call
        # this constructor with the wrong arguments.
        return (Fixed, (self._raw, self._frac_bits))

    def __copy__(self) -> Self:
        return self  # Immutable.

    def __deepcopy__(self, memo: object) -> Self:
        return self  # Immutable.


def raw_for(value: object, frac_bits: int) -> int:
    """The raw integer for `value` as `Fixed<frac_bits>`. Called by the extension."""
    if isinstance(value, Fixed) and value.frac_bits == frac_bits:
        return value.raw
    return Fixed.from_value(value, frac_bits).raw
