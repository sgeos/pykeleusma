"""Structs, enums, options and newtypes at the boundary.

A struct converts to a `dict` of its fields and back from any mapping with exactly
those fields. An enum value converts to `keleusma.Variant`, and a variant without
fields may be given as its name. An `Option<T>` is `None` or the value. A newtype
converts as its underlying type, except that a refined newtype is refused coming in,
because its rule is script code the host cannot run. The book covers these types in
its chapters on structs, enums and refinement types.
"""

import types

import pytest

import keleusma

Variant = keleusma.Variant
POINT = "struct Point { x: Word, y: Word }\n"
SHAPE = "enum Shape { Circle(Word), Rect(Word, Word), Empty }\n"
SWAP = keleusma.compile(
    POINT + "fn main(p: Point) -> Point { Point { x: p.y, y: p.x } }"
)
MORPH = keleusma.compile(
    SHAPE + "fn main(s: Shape) -> Shape { match s { "
    "Shape::Circle(r) => Shape::Rect(r, r), "
    "Shape::Rect(a, b) => Shape::Circle(a + b), "
    "Shape::Empty => Shape::Empty } }"
)
BUMP = keleusma.compile(
    "fn main(o: Option<Word>) -> Option<Word> { match o "
    "{ Option::Some(v) => Option::Some(v + 1), Option::None => Option::None } }"
)


def _call(module: keleusma.Module, *args: object) -> object:
    state = keleusma.VM(module).call(*args)
    assert isinstance(state, keleusma.Finished)
    return state.value


def test_a_struct_converts_to_and_from_a_dict() -> None:
    assert _call(SWAP, {"x": 1, "y": 2}) == {"x": 2, "y": 1}


def test_any_mapping_converts_to_a_struct() -> None:
    assert _call(SWAP, types.MappingProxyType({"y": 2, "x": 1})) == {"x": 2, "y": 1}


@pytest.mark.parametrize(
    ("value", "message"),
    [
        ({"x": 1}, "fields x, y"),
        ({"x": 1, "y": 2, "z": 3}, "fields x, y"),
        ((1, 2), "expected Point"),
        ({"x": 1, "y": "a"}, "expected Word"),
    ],
)
def test_a_mismatched_struct_is_refused(value: object, message: str) -> None:
    vm = keleusma.VM(SWAP)
    with pytest.raises(TypeError, match=message):
        vm.call(value)
    assert not vm.failed


@pytest.mark.parametrize(
    ("given", "expected"),
    [
        (Variant("Circle", (3,)), Variant("Rect", (3, 3))),
        (Variant("Rect", (1, 2)), Variant("Circle", (3,))),
        (Variant("Empty"), Variant("Empty")),
        ("Empty", Variant("Empty")),
    ],
)
def test_an_enum_converts_to_and_from_a_variant(
    given: object, expected: Variant
) -> None:
    assert _call(MORPH, given) == expected


@pytest.mark.parametrize(
    ("value", "message"),
    [
        (Variant("Nope"), "no variant `Nope`; its variants are Circle, Rect, Empty"),
        (Variant("Circle"), "takes 1 value"),
        ("Circle", "takes 1 value"),
        (5, "expected Shape"),
    ],
)
def test_a_mismatched_variant_is_refused(value: object, message: str) -> None:
    vm = keleusma.VM(MORPH)
    with pytest.raises(TypeError, match=message):
        vm.call(value)
    assert not vm.failed


def test_a_variant_matches_by_name_and_payload() -> None:
    match _call(MORPH, Variant("Circle", (2,))):
        case Variant("Rect", (a, b)):
            assert (a, b) == (2, 2)
        case other:
            pytest.fail(f"unexpected {other!r}")


def test_an_option_is_none_or_the_value() -> None:
    assert _call(BUMP, 1) == 2
    assert _call(BUMP, None) is None


def test_composites_nest() -> None:
    module = keleusma.compile(
        POINT + SHAPE + "fn main(x: Word) -> (Point, Option<Point>, [Shape; 2]) "
        "{ (Point { x: x, y: x }, Option::None, [Shape::Circle(x), Shape::Empty]) }"
    )
    point = {"x": 4, "y": 4}
    assert _call(module, 4) == (
        point,
        None,
        [Variant("Circle", (4,)), Variant("Empty")],
    )


def test_a_struct_with_a_tuple_and_text_field_converts() -> None:
    module = keleusma.compile(
        "struct In { p: (Word, Word), s: Text }\nfn main(i: In) -> Text { i.s }"
    )
    assert _call(module, {"p": (1, 2), "s": "text"}) == "text"


def test_a_stream_yields_a_struct_and_an_enum() -> None:
    module = keleusma.compile(
        POINT + "loop main(p: Point) -> Point { let q = yield p; q }"
    )
    vm = keleusma.VM(module)
    assert vm.call({"x": 1, "y": 2}) == keleusma.Yielded({"x": 1, "y": 2})
    assert vm.resume({"x": 3, "y": 4}) == keleusma.Reset()
    shapes = keleusma.compile(
        SHAPE + "loop main(x: Word) -> Shape { let n = yield Shape::Circle(x); n }"
    )
    assert keleusma.VM(shapes).call(4) == keleusma.Yielded(Variant("Circle", (4,)))


def test_natives_take_and_return_structs_enums_and_options() -> None:
    module = keleusma.compile(
        "use host::scale(Point) -> Point\n"
        "use host::shape(Word) -> Shape\n"
        "use host::maybe(Word) -> Option<Word>\n"
        + POINT
        + SHAPE
        + "fn main(x: Word) -> Word { "
        "let q = host::scale(Point { x: x, y: 1 }); "
        "let r = match host::shape(x) { Shape::Circle(r) => r, _ => 0 }; "
        "let m = match host::maybe(x) { Option::Some(v) => v, Option::None => 100 }; "
        "q.x + q.y + r + m }"
    )

    def scale(p: dict[str, int]) -> dict[str, int]:
        return {"x": p["x"] * 10, "y": p["y"] * 10}

    def shape(x: int) -> Variant | str:
        return Variant("Circle", (x,)) if x else "Empty"

    def maybe(x: int) -> int | None:
        return None if x > 3 else x

    natives = {"host::scale": scale, "host::shape": shape, "host::maybe": maybe}
    vm = keleusma.VM(module, natives=natives)
    assert vm.call(2) == keleusma.Finished(20 + 10 + 2 + 2)
    assert vm.call(5) == keleusma.Finished(50 + 10 + 5 + 100)


def test_a_plain_newtype_converts_as_its_underlying_type() -> None:
    module = keleusma.compile(
        "newtype Meters = Word;\n"
        "fn main(m: Meters) -> Meters { Meters((m as Word) + 1) }"
    )
    assert _call(module, 3) == 4


REFINED = (
    "fn nonneg(x: Word) -> bool { x >= 0 }\nnewtype Counter = Word where nonneg;\n"
)


def test_a_refined_newtype_may_leave_the_script() -> None:
    module = keleusma.compile(REFINED + "fn main(x: Word) -> Counter { Counter(x) }")
    assert _call(module, 3) == 3


def test_a_refined_newtype_is_refused_coming_in() -> None:
    module = keleusma.compile(REFINED + "fn main(c: Counter) -> Word { c as Word }")
    with pytest.raises(TypeError, match="refined newtype"):
        keleusma.VM(module).call(3)


@pytest.mark.parametrize(
    ("source", "message"),
    [
        ("fn main(o: Option<Option<Word>>) -> Word { 1 }", "nested option"),
        ("struct Box<T> { v: T }\nfn main(b: Box<Word>) -> Word { 1 }", "generic type"),
    ],
)
def test_unsupported_shapes_are_refused(source: str, message: str) -> None:
    vm = keleusma.VM(keleusma.compile(source))
    with pytest.raises(TypeError, match=message):
        vm.call(None)
    assert not vm.failed
