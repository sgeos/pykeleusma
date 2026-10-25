# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, tasks 1 to 8

**Goal.** Complete all tasks from 1 to 8, skipping those already done, then commit,
push and merge all work.

**Already done.** Tasks 1 to 7, merged through pull requests 1 to 10. Only task 8
remained.

**Done, task 8.** Three examples in `examples/`, each run by `tests/test_examples.py`.

- `batched_update_loop.py`. A hundred session lines on one thread, each resumed once per
  tick, with Dromon's tagged-tuple events and actions, and a tick budget that is the sum
  of the processors' `wcet_per_iteration`.
- `numpy_reduction.py`. A NumPy array's mean and variance, passed as lists of eight
  floats per iteration, matching `numpy.mean` and `numpy.var` in tests.
- `pygame_frame_loop.py`. A bouncing ball whose physics runs in Keleusma. The window is
  the documented manual check, and the tests run it headless with SDL's dummy drivers.

NumPy 2.5.4 and `pygame-ce` 2.5.8 joined the development dependencies. `pygame-ce` was
chosen over `pygame`, whose 2.6.1 release has no wheels past Python 3.13, while the
matrix tests 3.15.

**Deviation, for the operator.** The roadmap asks for a frame loop that "resumes once per
frame". Under Keleusma 0.2.2 that is not possible with a stream. Every stream must yield,
so a yield-free iteration is rejected, and an inner loop around a yield has no static
bound, so it is rejected too. Each iteration is therefore one yield and one `Reset`, and
the example advances one iteration per frame, which is two resumes. Calling `main` again
each frame also works, but restarting a suspended stream is behaviour I have not verified
as safe over many frames, so the example does not rely on it. The roadmap records this.

**Also, from `pydromon`.** Its notice `pydromon-20261025-data-segment-gap` reported that
private data reads `()` before assignment and that shared `data` blocks cannot be driven.
The first is Keleusma 0.2.2's documented behaviour, now pinned by a test. Its book says
otherwise, which went to `v0.2.3`. For the second, the binding now refuses such a module
at construction with a clear `VerifyError`. Exposing the shared buffer is future work.
Both are recorded under Data segments in the design, and `pydromon` was answered.

**Verification.** 193 Python tests and 27 Rust tests passed locally, with pyright in
strict mode covering the examples. All three examples were run from the command line.

**Not run.** The PyGame window itself, which needs a display. Run
`uv run python examples/pygame_frame_loop.py` to see it.

**Next.** Task 9, the statement of the guarantee boundary.
