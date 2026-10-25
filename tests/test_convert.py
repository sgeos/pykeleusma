"""Roadmap task 4. Values convert between Python and Keleusma by declared type.

Round trips go through a stream that yields its argument back, and through scripts
that build composites themselves, because only the latter make Keleusma produce the
flat byte bodies that the binding decodes by type. The shapes include every one in
the probe recorded for Dromon.
"""

from fractions import Fraction

import pytest

import keleusma


def _echo(declared: str, value: object, budget: int = 4096) -> object:
    source = f"loop main(e: {declared}) -> {declared} {{ let n = yield e; n }}"
    vm = keleusma.VM(keleusma.compile(source), argument_bytes=budget)
    state = vm.call(value)
    assert isinstance(state, keleusma.Yielded)
    return state.value


def _run(source: str, *args: object) -> keleusma.State:
    return keleusma.VM(keleusma.compile(source)).call(*args)


@pytest.mark.parametrize(
    ("declared", "value"),
    [
        ("Word", 0),
        ("Word", 2**63 - 1),
        ("Word", -(2**63)),
        ("Byte", 0),
        ("Byte", 255),
        ("bool", True),
        ("bool", False),
        ("Float", 1.5),
        ("Float", -0.0),
        ("Text", ""),
        ("Text", "héllo 日本"),
        ("()", None),
        ("(Text, Text)", ("tokens", "hello")),
        ("(Text, Word, Text)", ("tool", 7, "result")),
        ("((Word, bool), Text)", ((5, False), "x")),
        ("(Byte, bool, Word)", (7, True, -3)),
    ],
)
def test_a_value_round_trips_unchanged(declared: str, value: object) -> None:
    result = _echo(declared, value)
    assert result == value
    assert type(result) is type(value)


@pytest.mark.parametrize(
    ("declared", "value"),
    [
        ("[Word; 3]", [1, 2, 3]),
        ("[Text; 2]", ["a", "b"]),
        ("[(Text, Word); 2]", [("wake", 1), ("end_turn", 0)]),
    ],
)
def test_an_array_round_trips_as_a_list(declared: str, value: list[object]) -> None:
    assert _echo(declared, value) == value
    assert _echo(declared, tuple(value)) == value  # A tuple is accepted as input.


def test_an_integer_is_accepted_where_a_float_is_declared() -> None:
    result = _echo("Float", 3)
    assert result == 3.0
    assert type(result) is float


# Composites the script builds itself, which Keleusma stores as flat bodies.


def test_a_built_text_pair_decodes() -> None:
    source = (
        'loop main(e: (Text, Text)) -> (Text, Text) { let n = yield ("send", e.1); n }'
    )
    assert _run(source, ("tokens", "hello from host")) == keleusma.Yielded(
        ("send", "hello from host")
    )


def test_a_built_fixed_arity_action_decodes() -> None:
    source = (
        "loop main(e: Word) -> (Text, Word, Text) "
        '{ if e > 0 { yield ("send", 7, "x") } else { yield ("wake", 3, "") } }'
    )
    assert _run(source, 0) == keleusma.Yielded(("wake", 3, ""))
    assert _run(source, 1) == keleusma.Yielded(("send", 7, "x"))


def test_a_built_array_of_actions_decodes() -> None:
    source = (
        "loop main(e: Word) -> [(Text, Word); 2] "
        '{ yield [("wake", e), ("end_turn", 0)] }'
    )
    assert _run(source, 5) == keleusma.Yielded([("wake", 5), ("end_turn", 0)])


def test_a_built_nested_composite_decodes() -> None:
    source = (
        'fn main(w: Word) -> ((Word, Text), [bool; 2]) { ((w, "n"), [true, false]) }'
    )
    assert _run(source, 9) == keleusma.Finished(((9, "n"), [True, False]))


def test_a_host_tag_compares_equal_to_a_literal() -> None:
    source = (
        "loop main(e: (Text, Text)) -> Word "
        '{ if e.0 == "tokens" { yield 1 } else { yield 0 } }'
    )
    assert _run(source, ("tokens", "x")) == keleusma.Yielded(1)
    assert _run(source, ("wake", "x")) == keleusma.Yielded(0)


def test_several_arguments_convert_in_order() -> None:
    source = "fn main(a: Word, b: Text) -> (Text, Word) { (b, a + 1) }"
    assert _run(source, 41, "x") == keleusma.Finished(("x", 42))


@pytest.mark.parametrize(
    ("source", "yielded"),
    [
        # Same size as the declared (Text, Word). Reading it would dereference the
        # integer 3 as a text pointer and crash the process, which it did before
        # the binding added its own check.
        (
            "loop main(e: Word) -> (Text, Word) "
            '{ if e > 0 { yield ("a", 1) } else { yield (3, "wake") } }',
            '(3, "wake")',
        ),
        # A different size, which the length check would also catch.
        (
            "loop main(e: Word) -> (Text, Text) "
            '{ if e > 0 { yield ("send", "x") } else { yield ("wake", 3) } }',
            '("wake", 3)',
        ),
        (
            'loop main(e: Word) -> Text { if e > 0 { yield "a" } else { yield 3 } }',
            "3",
        ),
    ],
)
def test_a_mistyped_yield_is_rejected_at_compile_time(
    source: str, yielded: str
) -> None:
    # Keleusma 0.2.2 accepts these, as reported to its maintainers. The binding
    # compares each yield's inferred type with the declared one and refuses the
    # module, pointing at the offending value.
    with pytest.raises(keleusma.CompileError, match="declared type is") as caught:
        keleusma.compile(source)
    assert caught.value.lineno == 1
    assert caught.value.colno == source.index(yielded) + 1


# Fixed-point values.


def test_a_fixed_round_trips_exactly() -> None:
    value = keleusma.Fixed.from_value(Fraction(-3, 1024), 16)
    result = _echo("Fixed<16>", value)
    assert isinstance(result, keleusma.Fixed)
    assert (result.raw, result.frac_bits) == (value.raw, value.frac_bits)


def test_the_extreme_raw_values_round_trip() -> None:
    for raw in (-(2**63), 2**63 - 1):
        result = _echo("Fixed<62>", keleusma.Fixed(raw, 62))
        assert isinstance(result, keleusma.Fixed)
        assert result.raw == raw


def test_a_bare_fixed_takes_the_host_default_of_32_bits() -> None:
    result = _echo("Fixed", Fraction(1, 4))
    assert isinstance(result, keleusma.Fixed)
    assert result.frac_bits == 32
    assert result == Fraction(1, 4)


def test_any_exact_number_converts_into_a_fixed() -> None:
    for value in (2, 2.25, Fraction(9, 4), keleusma.Fixed(9, 2)):
        result = _echo("Fixed<16>", value)
        assert result == Fraction(value)


def test_fixed_arithmetic_in_the_script_is_exact() -> None:
    source = "loop main(e: Fixed<16>) -> Fixed<16> { yield e + e }"
    assert _run(source, Fraction(3, 4)) == keleusma.Yielded(Fraction(3, 2))


def test_an_inexact_fixed_is_refused_rather_than_rounded() -> None:
    with pytest.raises(ValueError, match="not exactly representable"):
        _echo("Fixed<16>", 0.1)


def test_a_fixed_too_large_for_64_bits_is_refused() -> None:
    with pytest.raises(OverflowError):
        _echo("Fixed<62>", 2)


def test_tuple_arguments_convert_in_a_loop() -> None:
    source = (
        "loop main(e: (Text, Fixed<8>)) -> (Text, Fixed<8>) "
        "{ let n = yield (e.0, e.1 + e.1); n }"
    )
    assert _run(source, ("half", Fraction(1, 2))) == keleusma.Yielded(("half", 1))


# Values that must be refused.


@pytest.mark.parametrize("value", [2**63, -(2**63) - 1])
def test_a_word_outside_64_bits_raises_overflow_error(value: int) -> None:
    with pytest.raises(OverflowError):
        _echo("Word", value)


@pytest.mark.parametrize("value", [256, -1, 2**70])
def test_a_byte_outside_0_to_255_raises_overflow_error(value: int) -> None:
    with pytest.raises(OverflowError, match="out of range for Byte"):
        _echo("Byte", value)


@pytest.mark.parametrize(
    ("declared", "value"),
    [
        ("Word", True),
        ("Word", "1"),
        ("Word", 1.0),
        ("Byte", False),
        ("bool", 1),
        ("Float", "1.0"),
        ("Float", True),
        ("Text", b"bytes"),
        ("Text", 1),
        ("()", 0),
        ("(Text, Text)", ["a", "b"]),
        ("(Text, Text)", ("a",)),
        ("(Text, Text)", ("a", 1)),
        ("[Word; 3]", [1, 2]),
        ("[Word; 3]", "abc"),
        ("Fixed<8>", "1"),
        ("Fixed<8>", True),
    ],
)
def test_a_value_of_the_wrong_type_raises_type_error(
    declared: str, value: object
) -> None:
    with pytest.raises(TypeError):
        _echo(declared, value)


def test_the_wrong_number_of_arguments_raises_type_error() -> None:
    vm = keleusma.VM(keleusma.compile("fn main(a: Word, b: Word) -> Word { a + b }"))
    with pytest.raises(TypeError, match="takes 2 argument"):
        vm.call(1)


def test_an_unsupported_parameter_type_names_the_type() -> None:
    source = "struct P { x: Word }\nfn main(p: P) -> Word { p.x }"
    vm = keleusma.VM(keleusma.compile(source))
    with pytest.raises(TypeError, match="named type `P`"):
        vm.call(1)


def test_an_argument_larger_than_the_budget_raises_vm_error() -> None:
    with pytest.raises(keleusma.VMError, match="OutOfArena"):
        _echo("(Text, Text)", ("tokens", "x" * 8192), budget=64)


def test_a_failed_conversion_leaves_the_vm_usable() -> None:
    source = "loop main(e: Word) -> Word { let n = yield e; n }"
    vm = keleusma.VM(keleusma.compile(source))
    with pytest.raises(TypeError):
        vm.call("not a word")
    assert vm.call(5) == keleusma.Yielded(5)
