"""Roadmap task 8. The examples run, and their results are checked.

Each example is imported as a module and driven through the function it exposes, so a
test checks its behaviour rather than only that it starts. The PyGame example runs
with SDL's dummy video and audio drivers, so it needs no display.
"""

import numpy as np
import pytest
from batched_update_loop import Scheduler
from numpy_reduction import mean_and_variance
from pygame_frame_loop import Ball, run

# The batched update loop, in Dromon's event and action shape.


def test_each_line_acts_on_its_own_event() -> None:
    scheduler = Scheduler(count=100)
    assert scheduler.tick({3: ("message", "review my patch"), 7: ("tokens", "x")}) == [
        ("wake", 3, "review my patch")
    ]
    assert scheduler.tick({7: ("turn_end", "")}) == [("end_turn", 7, "")]
    assert scheduler.tick({}) == []


def test_a_hundred_lines_run_many_ticks_on_one_thread() -> None:
    scheduler = Scheduler(count=100)
    for tick in range(50):
        events = {line: ("message", f"tick {tick}") for line in range(0, 100, 10)}
        actions = scheduler.tick(events)
        assert actions == [("wake", line, f"tick {tick}") for line in range(0, 100, 10)]
    assert not any(line.vm.failed for line in scheduler.lines)


def test_the_tick_budget_is_the_sum_of_the_processors_figures() -> None:
    scheduler = Scheduler(count=100)
    per_line = scheduler.lines[0].vm.wcet_per_iteration
    assert per_line is not None
    assert scheduler.tick_budget() == 100 * per_line


# The NumPy reduction, passing lists.


@pytest.mark.parametrize("size", [1, 7, 8, 9, 1_000])
def test_the_reduction_matches_numpy(size: int) -> None:
    values = np.random.default_rng(seed=size).normal(loc=3.0, scale=2.0, size=size)
    mean, variance = mean_and_variance(values)
    assert mean == pytest.approx(float(values.mean()), rel=1e-12, abs=1e-12)
    assert variance == pytest.approx(float(values.var()), rel=1e-9, abs=1e-12)


@pytest.mark.parametrize(
    "values", [np.array([], dtype=np.float64), np.zeros((2, 8), dtype=np.float64)]
)
def test_the_reduction_refuses_empty_or_multidimensional_input(
    values: np.ndarray[tuple[int, ...], np.dtype[np.float64]],
) -> None:
    with pytest.raises(ValueError, match="one-dimensional"):
        mean_and_variance(values)


# The PyGame frame loop.


def test_the_ball_stays_in_bounds_and_bounces() -> None:
    ball = Ball(position=0.5, velocity=0.8)
    velocities: list[float] = []
    for _ in range(100):
        position, velocity = ball.advance(0.1)
        assert 0.0 <= position <= 1.0
        velocities.append(velocity)
    assert min(velocities) < 0 < max(velocities)  # It reflected off a wall.


def test_one_frame_advances_the_stream_by_one_iteration() -> None:
    ball = Ball(position=0.0, velocity=1.0)
    positions = [ball.advance(0.25)[0] for _ in range(3)]
    assert positions == [0.25, 0.5, 0.75]


def test_the_frame_loop_runs_headless(monkeypatch: pytest.MonkeyPatch) -> None:
    # pygame-ce has no free-threaded build, so the free-threaded CI job lacks it. The
    # other jobs install it, and their counts show the test ran there.
    pytest.importorskip("pygame")
    monkeypatch.setenv("SDL_VIDEODRIVER", "dummy")
    monkeypatch.setenv("SDL_AUDIODRIVER", "dummy")
    assert run(frames=60, fps=0) == 60
