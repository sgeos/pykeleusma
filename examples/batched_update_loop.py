"""Many session lines on one thread, each resumed once per tick, in Dromon's shape.

    uv run python examples/batched_update_loop.py

Each session line has its own stream processor, a VM built from one shared module. An
event is a tuple `(tag, line, text)` and an action a tuple `(kind, line, text)`, with
the empty string as padding, because Keleusma 0.2.2 rejects one-element tuples and
every yield of a processor shares one type. The processor dispatches on the event's
tag by comparing it with literal strings, which Keleusma supports without natives.

**The tick budget.** Every processor advances by exactly one stream iteration per tick,
so the script work of a tick is bounded by the sum of the processors'
`wcet_per_iteration`, in Keleusma's nominal cost units. Python's own work, including
the binding's conversions and acting on the actions, is outside that bound.
"""

from collections.abc import Mapping
from dataclasses import dataclass

import keleusma

#: A wake when another line sends a message, an end of turn when the model finishes,
#: and nothing otherwise.
PROCESSOR = """
loop main(e: (Text, Word, Text)) -> (Text, Word, Text) {
    let action = if e.0 == "message" {
        ("wake", e.1, e.2)
    } else {
        if e.0 == "turn_end" { ("end_turn", e.1, "") } else { ("none", e.1, "") }
    };
    let n = yield action;
    n
}
"""

type Event = tuple[str, int, str]
type Action = tuple[str, int, str]

#: The event a line receives in a tick with nothing for it.
IDLE: tuple[str, str] = ("idle", "")


@dataclass
class Line:
    """One session line and its stream processor."""

    number: int
    vm: keleusma.VM
    started: bool = False

    def advance(self, event: Event) -> Action:
        """Run one iteration of the processor on `event` and return its action."""
        if not self.started:
            state = self.vm.call(event)
            self.started = True
        else:
            state = self.vm.resume(event)
            if isinstance(state, keleusma.Reset):
                state = self.vm.resume(event)
        match state:
            case keleusma.Yielded((str() as kind, int() as line, str() as text)):
                return (kind, line, text)
            case _:
                raise RuntimeError(f"expected a yielded action, got {state!r}")


class Scheduler:
    """Advances every line once per tick, in a fixed order, on the calling thread."""

    def __init__(self, count: int) -> None:
        module = keleusma.compile(PROCESSOR)
        self.lines = [
            Line(number, keleusma.VM(module, argument_bytes=512))
            for number in range(count)
        ]

    def tick_budget(self) -> int:
        """The worst-case script work of one tick, in nominal cost units."""
        total = 0
        for line in self.lines:
            figure = line.vm.wcet_per_iteration
            if figure is None:
                raise RuntimeError("every processor must be a loop main")
            total += figure
        return total

    def tick(self, events: Mapping[int, tuple[str, str]]) -> list[Action]:
        """Give each line its event for this tick, or an idle one, and collect the
        actions that are not `none`."""
        actions: list[Action] = []
        for line in self.lines:
            tag, text = events.get(line.number, IDLE)
            action = line.advance((tag, line.number, text))
            if action[0] != "none":
                actions.append(action)
        return actions


def main() -> None:
    scheduler = Scheduler(count=100)
    print(f"tick budget: {scheduler.tick_budget()} nominal cost units for 100 lines")
    ticks: list[dict[int, tuple[str, str]]] = [
        {3: ("message", "review my patch"), 7: ("tokens", "thinking")},
        {7: ("turn_end", "")},
        {},
    ]
    for number, events in enumerate(ticks):
        print(f"tick {number}: {scheduler.tick(events)}")


if __name__ == "__main__":
    main()
