"""Exceptions raised by the keleusma package.

The classes live in Python rather than in the extension so that a source error can
derive from both `KeleusmaError` and `ValueError`, which PyO3's exception macros cannot
express. The extension raises them through `pyo3::import_exception!`.
"""


class KeleusmaError(Exception):
    """Base class for every error the keleusma package raises."""


class SourceError(KeleusmaError, ValueError):
    """Source text that Keleusma rejected, with the location of the fault.

    The attributes follow `json.JSONDecodeError`. `lineno` and `colno` are 1-based.
    `colno` counts characters, as Python does, not the bytes Keleusma counts. Either
    is `None` when Keleusma reports no location, which happens for errors in types the
    compiler synthesises rather than reads from the source.

    It derives from `ValueError` because the caller passed a value, the source text,
    that is malformed, as `json.JSONDecodeError` does.
    """

    msg: str
    lineno: int | None
    colno: int | None

    def __init__(self, msg: str, lineno: int | None, colno: int | None) -> None:
        if lineno is None or colno is None:
            text = msg
        else:
            text = f"{msg} (line {lineno}, column {colno})"
        super().__init__(text)
        self.msg = msg
        self.lineno = lineno
        self.colno = colno

    def __reduce__(
        self,
    ) -> tuple[type["SourceError"], tuple[str, int | None, int | None]]:
        # Exceptions pickle through their constructor arguments, which here differ
        # from `args`, so without this a pickled error could not be rebuilt.
        return (type(self), (self.msg, self.lineno, self.colno))


class LexError(SourceError):
    """The lexer rejected the source, for example on a character outside the grammar."""


class ParseError(SourceError):
    """The parser rejected the token stream."""


class CompileError(SourceError):
    """The compiler rejected a program that parsed, for example on a type error."""
