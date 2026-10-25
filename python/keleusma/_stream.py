"""`vm.stream(...)`, a VM driven through Python's generator protocol.

The generator yields every `Yielded` and `Reset` state and returns the `Finished`
value, so `result = yield from vm.stream(...)` or `StopIteration.value` receives it.
Nothing runs until the generator is first advanced, as with any Python generator.

Each resume takes its input from one of two places.

- **`inputs`.** An iterable of resume values. The stream ends, returning `None`, when it
  is exhausted, which suits a plain `for` loop over recorded or computed events.
- **`send`.** Without `inputs`, the value passed to `generator.send(value)` is the next
  input. `next(generator)` sends `None`, which converts only where the script declares
  its parameter as `()`, so a `for` loop without `inputs` suits exactly those streams.

**The meaning of `Reset`.** A `loop main` never finishes. After each iteration it
reports `Reset`, and the next input starts the next iteration as its argument. The
generator yields `keleusma.Reset()` at that point, so the consumer sees the iteration
boundary and supplies, or lets `inputs` supply, the argument for the next one.

Leaving the loop early, with `break` or by discarding the generator, leaves the VM
suspended where it was, and it can still be resumed directly. An exception from the
VM ends the generator and propagates, and a runtime error marks the VM failed.
"""

from collections.abc import Generator, Iterable
from typing import Protocol

from keleusma._state import Finished, Reset, Yielded


class _Resumable(Protocol):
    def call(self, *args: object) -> Yielded | Finished | Reset: ...
    def resume(self, value: object) -> Yielded | Finished | Reset: ...


def stream(
    vm: _Resumable, *args: object, inputs: Iterable[object] | None = None
) -> Generator[Yielded | Reset, object, object]:
    """Call `vm` with `args` and resume it until it finishes or `inputs` runs out."""
    feed = None if inputs is None else iter(inputs)
    state = vm.call(*args)
    while not isinstance(state, Finished):
        sent = yield state
        if feed is None:
            value = sent
        else:
            if sent is not None:
                raise TypeError(
                    "this stream takes its inputs from `inputs`, so send() is not used"
                )
            try:
                value = next(feed)
            except StopIteration:
                return None
        state = vm.resume(value)
    return state.value
