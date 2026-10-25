"""The states a VM reports after `call` or `resume`.

Each is a frozen dataclass, so a result can be compared with `==` and taken apart with
`match`, which is the Python idiom for a closed set of variants.

    match vm.resume(event):
        case keleusma.Yielded(actions):
            perform(actions)
        case keleusma.Reset():
            pass  # The stream finished one iteration; resume starts the next.
        case keleusma.Finished(result):
            done(result)
"""

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class Yielded:
    """The script yielded `value` and is suspended until `resume`."""

    value: object


@dataclass(frozen=True, slots=True)
class Finished:
    """The script returned `value`. The VM cannot be resumed again."""

    value: object


@dataclass(frozen=True, slots=True)
class Reset:
    """A `loop main` finished one iteration. The next `resume` starts another."""


#: Any state `call` or `resume` returns.
type State = Yielded | Finished | Reset
