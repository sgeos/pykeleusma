"""Conversion of Keleusma's `Multiword<N, F>` big numbers, the book's chapter 23.

A `Multiword<N, F>` is `N` 64-bit words, least significant first, holding a signed
two's complement integer `raw`, and its value is `raw / 2**F`. Keleusma compares
multiwords as signed, which a probe confirmed. With `F` zero the value converts to
an `int`, and otherwise to an exact `Fraction`. Coming in, any exact number converts
if it is representable, by the rules `keleusma.Fixed.from_value` uses, and a value
out of range raises `OverflowError` rather than wrapping.
"""

from fractions import Fraction

from keleusma._fixed import scaled_exactly

WORD_BYTES = 8


def to_words(value: object, words: int, frac_bits: int) -> list[int]:
    """The `words` signed 64-bit words that represent `value`, low word first."""
    if isinstance(value, int) and not isinstance(value, bool) and frac_bits == 0:
        raw = value
    else:
        raw = scaled_exactly(value, frac_bits)
    try:
        data = raw.to_bytes(words * WORD_BYTES, "little", signed=True)
    except OverflowError:
        raise OverflowError(
            f"{value!r} does not fit Multiword<{words}, {frac_bits}>"
        ) from None
    return [
        int.from_bytes(data[i : i + WORD_BYTES], "little", signed=True)
        for i in range(0, len(data), WORD_BYTES)
    ]


def from_words(words: list[int], frac_bits: int) -> int | Fraction:
    """The value that `words`, low word first, represent with `frac_bits`."""
    data = b"".join(w.to_bytes(WORD_BYTES, "little", signed=True) for w in words)
    raw = int.from_bytes(data, "little", signed=True)
    return raw if frac_bits == 0 else Fraction(raw, 1 << frac_bits)
