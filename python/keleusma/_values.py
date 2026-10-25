"""Python forms of Keleusma's structs and enums.

A struct converts to a `dict` of its fields in declaration order, and any mapping with
exactly those fields converts back. An enum value converts to a `Variant`, and a
`Variant` or, for a variant without fields, its name as a string converts back. An
`Option<T>` converts to `None` or to the value itself, Python's own idiom for an
optional value.
"""

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class Variant:
    """One value of a Keleusma enum, its variant's name and its payload.

    It works with `match`, for example `case Variant("Send", (line, text)):`, and
    compares equal to a `Variant` with the same name and payload.
    """

    name: str
    values: tuple[object, ...] = ()
