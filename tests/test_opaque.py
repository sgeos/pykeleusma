"""Opaque host values, which the Keleusma book's embedding guide describes.

A script that names a type it does not define treats it as an opaque host type. Any
Python object passes where such a type is declared, and the same object comes back.
"""

import gc
import weakref

import keleusma


class Sprite:
    """A host object the script carries without looking inside."""

    def __init__(self, n: int) -> None:
        self.n = n


def _value(state: keleusma.State) -> object:
    assert isinstance(state, (keleusma.Finished, keleusma.Yielded))
    return state.value


def test_the_same_object_comes_back() -> None:
    sprite = Sprite(1)
    module = keleusma.compile("fn main(s: Sprite) -> Sprite { s }")
    assert _value(keleusma.VM(module).call(sprite)) is sprite


def test_an_opaque_value_travels_inside_composites() -> None:
    sprite = Sprite(1)
    pair = keleusma.compile("fn main(s: Sprite, n: Word) -> (Sprite, Word) { (s, n) }")
    result = _value(keleusma.VM(pair).call(sprite, 2))
    assert isinstance(result, tuple)
    assert result[0] is sprite  # pyright: ignore[reportUnknownMemberType]
    held = keleusma.compile(
        "struct Held { s: Sprite, n: Word }\nfn main(h: Held) -> Held { h }"
    )
    assert _value(keleusma.VM(held).call({"s": sprite, "n": 3})) == {
        "s": sprite,
        "n": 3,
    }
    both = keleusma.compile("fn main(s: Sprite) -> [Sprite; 2] { [s, s] }")
    assert _value(keleusma.VM(both).call(sprite)) == [sprite, sprite]


def test_a_stream_yields_an_opaque_value() -> None:
    sprite = Sprite(1)
    module = keleusma.compile("loop main(s: Sprite) -> Sprite { let n = yield s; n }")
    vm = keleusma.VM(module)
    assert _value(vm.call(sprite)) is sprite


def test_natives_create_and_consume_opaque_values() -> None:
    # The book's example, with Python strings standing in for its Rust strings.
    module = keleusma.compile(
        "use make_string() -> RustString\n"
        "use upper_case(RustString) -> RustString\n"
        "fn main() -> RustString { let s = make_string(); upper_case(s) }"
    )

    def make_string() -> str:
        return "hello"

    def upper_case(s: str) -> str:
        return s.upper()

    natives = {"make_string": make_string, "upper_case": upper_case}
    assert keleusma.VM(module, natives=natives).call() == keleusma.Finished("HELLO")


def test_a_native_receives_an_opaque_field_of_a_flat_composite() -> None:
    sprite = Sprite(4)
    module = keleusma.compile(
        "use host::f((Sprite, Word)) -> Word\n"
        "fn main(s: Sprite) -> Word { host::f((s, 5)) }"
    )

    def f(pair: tuple[Sprite, int]) -> int:
        return pair[0].n + pair[1]

    assert keleusma.VM(module, natives={"host::f": f}).call(
        sprite
    ) == keleusma.Finished(9)


def test_each_crossing_is_a_new_value_to_the_script() -> None:
    # Keleusma compares opaques by the identity of their wrapper, and each crossing
    # wraps the object anew, as src/opaque.rs records.
    sprite = Sprite(1)
    module = keleusma.compile("fn main(a: Sprite, b: Sprite) -> bool { a == b }")
    assert keleusma.VM(module).call(sprite, sprite) == keleusma.Finished(False)


def test_the_vm_releases_the_object() -> None:
    sprite = Sprite(1)
    ref = weakref.ref(sprite)
    module = keleusma.compile("fn main(s: Sprite) -> Word { 0 }")
    vm = keleusma.VM(module)
    vm.call(sprite)
    del sprite, vm
    gc.collect()
    assert ref() is None


def test_a_native_returns_an_opaque_value() -> None:
    # An opaque value carries its type name, so it converts out without one.
    module = keleusma.compile(
        "use host::make() -> Sprite\nfn main() -> Sprite { host::make() }"
    )
    sprite = Sprite(1)

    def make() -> Sprite:
        return sprite

    vm = keleusma.VM(module, natives={"host::make": make})
    assert _value(vm.call()) is sprite
