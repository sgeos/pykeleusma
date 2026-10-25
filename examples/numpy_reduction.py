"""The mean and variance of a NumPy array, reduced in Keleusma eight samples at a time.

    uv run python examples/numpy_reduction.py

The array crosses the boundary as Python lists of floats, eight per stream iteration.
The script adds each chunk to a running sum and sum of squares, which the host passes
back in with the next chunk, so every iteration is a bounded
piece of work and the reduction holds no hidden state. The last chunk is padded with
zeros, which change neither sum.

Keleusma bindings are immutable, so the script accumulates in its private data segment,
and it walks the chunk with `for i in 0..8`, the loop form its verifier can bound.
"""

import numpy as np
import numpy.typing as npt

import keleusma

#: The number of samples per iteration, fixed by the script's `[Float; 8]` parameter.
CHUNK = 8

SCRIPT = """
private data acc { sum: Float, squares: Float }
loop main(e: ([Float; 8], Float, Float)) -> (Float, Float) {
    acc.sum = e.1;
    acc.squares = e.2;
    for i in 0..8 {
        let x = e.0[i];
        acc.sum = acc.sum + x;
        acc.squares = acc.squares + x * x;
    }
    let n = yield (acc.sum, acc.squares);
    n
}
"""


def mean_and_variance(
    values: npt.NDArray[np.float64], vm: keleusma.VM | None = None
) -> tuple[float, float]:
    """The population mean and variance of a non-empty one-dimensional array."""
    if values.ndim != 1 or values.size == 0:
        raise ValueError("expected a non-empty one-dimensional array")
    machine = vm if vm is not None else keleusma.VM(keleusma.compile(SCRIPT))
    padded = np.zeros(-(-values.size // CHUNK) * CHUNK, dtype=np.float64)
    padded[: values.size] = values
    total, squares = 0.0, 0.0
    started = False
    for chunk in padded.reshape(-1, CHUNK):
        samples = [float(sample) for sample in chunk]  # Python floats, as a list.
        step_input = (samples, total, squares)
        if not started:
            state = machine.call(step_input)
            started = True
        else:
            state = machine.resume(step_input)
            if isinstance(state, keleusma.Reset):
                state = machine.resume(step_input)
        match state:
            case keleusma.Yielded((float() as new_total, float() as new_squares)):
                total, squares = new_total, new_squares
            case _:
                raise RuntimeError(f"expected yielded sums, got {state!r}")
    mean = total / values.size
    variance = squares / values.size - mean * mean
    return mean, variance


def main() -> None:
    values = np.random.default_rng(seed=2026).normal(loc=3.0, scale=2.0, size=1_000)
    vm = keleusma.VM(keleusma.compile(SCRIPT))
    mean, variance = mean_and_variance(values, vm)
    print(f"Keleusma: mean {mean:.6f}, variance {variance:.6f}")
    print(f"NumPy:    mean {values.mean():.6f}, variance {values.var():.6f}")
    print(f"worst case per chunk: {vm.wcet_per_iteration} nominal cost units")


if __name__ == "__main__":
    main()
