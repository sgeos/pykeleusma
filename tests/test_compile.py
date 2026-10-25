"""Roadmap task 2. `keleusma.compile` returns a module or raises a located error.

Expected locations are derived from the source text with `str.index`, so each test
states where the fault is rather than pinning a number that happens to match.
Character columns are what Python counts, so a test with non-ASCII text before the
fault would fail if the binding passed Keleusma's byte column through unchanged.
"""

import pickle
import threading

import pytest

import keleusma


def _column_of(line: str, needle: str) -> int:
    """The 1-based character column of the first `needle` in `line`."""
    return line.index(needle) + 1


def test_valid_source_compiles_to_a_module() -> None:
    module = keleusma.compile("fn main() -> Word { 1 }")
    assert isinstance(module, keleusma.Module)
    assert "1 chunk" in repr(module)


def test_empty_source_compiles_to_a_module_with_no_chunks() -> None:
    # Keleusma accepts an empty program. Rejecting it, for want of an entry point,
    # is the VM's concern in task 3, not the compiler's.
    assert "0 chunk" in repr(keleusma.compile(""))


@pytest.mark.parametrize(
    ("source", "needle", "error"),
    [
        ("fn main() -> Word { $ }", "$", keleusma.LexError),
        ('fn main() -> Text { "abc }', '"', keleusma.LexError),
        ("fn main() -> Word { @ }", "@", keleusma.ParseError),
        ("fn main() -> Word { x }", "x", keleusma.CompileError),
    ],
)
def test_each_stage_raises_its_own_error_with_a_location(
    source: str, needle: str, error: type[keleusma.SourceError]
) -> None:
    with pytest.raises(error) as caught:
        keleusma.compile(source)
    assert caught.value.lineno == 1
    assert caught.value.colno == _column_of(source, needle)
    assert caught.value.msg
    assert f"(line 1, column {caught.value.colno})" in str(caught.value)


def test_a_fault_on_a_later_line_reports_that_line() -> None:
    lines = ["fn main() -> Word {", "    1 +", "}"]
    with pytest.raises(keleusma.ParseError) as caught:
        keleusma.compile("\n".join(lines))
    assert caught.value.lineno == 3
    assert caught.value.colno == _column_of(lines[2], "}")


def test_the_column_counts_characters_not_bytes() -> None:
    # "日本" is two characters and six bytes. Keleusma reports a byte column, which
    # would put the '$' four places too far right.
    source = 'fn main() -> Text { "日本" $ }'
    with pytest.raises(keleusma.LexError) as caught:
        keleusma.compile(source)
    assert caught.value.colno == _column_of(source, "$")
    byte_column = len(source[: source.index("$")].encode()) + 1
    assert caught.value.colno != byte_column


def test_the_character_column_holds_on_a_later_line() -> None:
    lines = ["fn main() -> Word {", '    let s = "日本"; zzz', "}"]
    with pytest.raises(keleusma.CompileError) as caught:
        keleusma.compile("\n".join(lines))
    assert caught.value.lineno == 2
    assert caught.value.colno == _column_of(lines[1], "zzz")


def test_source_errors_form_one_hierarchy_under_value_error() -> None:
    for error in (keleusma.LexError, keleusma.ParseError, keleusma.CompileError):
        assert issubclass(error, keleusma.SourceError)
    assert issubclass(keleusma.SourceError, keleusma.KeleusmaError)
    assert issubclass(keleusma.SourceError, ValueError)
    with pytest.raises(ValueError, match="unexpected character"):
        keleusma.compile("fn main() -> Word { $ }")


def test_a_source_error_without_a_location_omits_it_from_the_message() -> None:
    # Keleusma reports line 0 for synthesised types. The extension maps that to
    # None, which src/compile.rs tests, and the message then carries no location.
    error = keleusma.CompileError("type error", None, None)
    assert str(error) == "type error"
    assert error.lineno is None
    assert error.colno is None


def test_a_source_error_survives_pickling() -> None:
    with pytest.raises(keleusma.ParseError) as caught:
        keleusma.compile("fn main() -> Word { @ }")
    # Pickling exercises SourceError.__reduce__, so the round trip is not
    # arbitrary data. The value comes from this process.
    restored = pickle.loads(pickle.dumps(caught.value))  # noqa: S301
    assert type(restored) is keleusma.ParseError
    assert (restored.msg, restored.lineno, restored.colno) == (
        caught.value.msg,
        caught.value.lineno,
        caught.value.colno,
    )
    assert str(restored) == str(caught.value)


def test_a_non_string_source_is_a_type_error() -> None:
    with pytest.raises(TypeError):
        keleusma.compile(123)  # pyright: ignore[reportArgumentType]


def test_modules_compile_concurrently_and_cross_threads() -> None:
    # Module is frozen and Keleusma's Module is Send and Sync, so modules may be
    # compiled on several threads and handed to another.
    results: list[keleusma.Module] = []
    errors: list[BaseException] = []

    def work(value: int) -> None:
        try:
            results.append(keleusma.compile(f"fn main() -> Word {{ {value} }}"))
        except BaseException as error:
            errors.append(error)

    threads = [threading.Thread(target=work, args=(n,)) for n in range(8)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert errors == []
    assert len(results) == 8
    assert all("1 chunk" in repr(module) for module in results)
