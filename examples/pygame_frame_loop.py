"""A bouncing ball whose physics runs in Keleusma, one stream iteration per frame.

Run it in a window, and close the window to stop.

    uv run python examples/pygame_frame_loop.py

Run it without a display for a fixed number of frames, as the tests do.

    SDL_VIDEODRIVER=dummy uv run python examples/pygame_frame_loop.py --frames 300

**One iteration per frame.** A Keleusma 0.2.2 stream must yield, and its verifier
rejects an inner loop without a static bound, so each iteration of a `loop main` is one
yield followed by a `Reset`. Each frame advances the stream by one iteration with a
single `vm.step(frame_input)`, which completes the previous iteration through its
`Reset` and yields this frame's result. The script's own work per frame is bounded by
`vm.wcet_per_iteration`, while drawing and the rest of the frame are Python.
"""

import argparse
import os

import keleusma

#: Position and velocity in the unit interval, reflected at both walls. The input is
#: (seconds since the last frame, position, velocity), and the output the new
#: position and velocity. State lives in the host, so each iteration is independent.
SCRIPT = """
loop main(e: (Float, Float, Float)) -> (Float, Float) {
    let x = e.1 + e.2 * e.0;
    let low = x < 0.0;
    let high = x > 1.0;
    let position = if low { 0.0 - x } else { if high { 2.0 - x } else { x } };
    let velocity = if low { 0.0 - e.2 } else { if high { 0.0 - e.2 } else { e.2 } };
    let n = yield (position, velocity);
    n
}
"""


class Ball:
    """The ball's state, advanced by one stream iteration per frame."""

    def __init__(self, position: float = 0.5, velocity: float = 0.8) -> None:
        self.vm = keleusma.VM(keleusma.compile(SCRIPT))
        self.position = position
        self.velocity = velocity

    def advance(self, seconds: float) -> tuple[float, float]:
        """Run one iteration with the frame's elapsed time and return the new state."""
        state = self.vm.step((seconds, self.position, self.velocity))
        match state:
            case keleusma.Yielded((float() as position, float() as velocity)):
                self.position, self.velocity = position, velocity
            case _:
                raise RuntimeError(f"expected a yielded position, got {state!r}")
        return self.position, self.velocity


def run(frames: int | None = None, fps: int = 60) -> int:
    """Open a window and animate the ball. Return the number of frames drawn."""
    import pygame  # Imported here so the module loads without pygame installed.

    pygame.init()
    try:
        width, height, radius = 480, 120, 12
        screen = pygame.display.set_mode((width, height))
        pygame.display.set_caption("pykeleusma bouncing ball")
        clock = pygame.time.Clock()
        ball = Ball()
        drawn = 0
        running = True
        while running and (frames is None or drawn < frames):
            for event in pygame.event.get():
                if event.type == pygame.QUIT:
                    running = False
            seconds = clock.tick(fps) / 1000.0
            position, _ = ball.advance(seconds)
            screen.fill((20, 20, 30))
            x = radius + round(position * (width - 2 * radius))
            pygame.draw.circle(screen, (230, 180, 60), (x, height // 2), radius)
            pygame.display.flip()
            drawn += 1
        return drawn
    finally:
        pygame.quit()


def main() -> None:
    parser = argparse.ArgumentParser(description="Animate a Keleusma bouncing ball.")
    parser.add_argument("--frames", type=int, default=None, help="stop after N frames")
    arguments = parser.parse_args()
    frames: int | None = arguments.frames
    drawn = run(frames)
    driver = os.environ.get("SDL_VIDEODRIVER", "the default video driver")
    print(f"drew {drawn} frames with {driver}")


if __name__ == "__main__":
    main()
