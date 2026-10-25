"""Fuzz `keleusma.load_module` and VM construction with mutated modules.

Each case compiles one of a few small programs, mutates its wire bytes, usually
repairs the CRC-32 trailer so the mutation reaches Keleusma's decoder rather than
stopping at the checksum, then loads, builds and runs the result. Cases run in child
processes in batches, so a crash ends only its batch, and its seed is reported.

Run it after any Keleusma upgrade, for example `uv run python tools/fuzz_load_module.py
--batches 300`, which runs 300,000 cases. Every outcome other than a crash is
acceptable, because a refusal raises a Python exception. On 2026-10-25 it found one
crash, a stack overflow in Keleusma's footprint analysis reached before verification,
now prevented in `src/owned_vm.rs`, and none in a following run of 300,000 cases.
"""

import argparse
import collections
import random
import subprocess
import sys
import zlib

import keleusma

SOURCES = [
    "fn main(x: Word) -> Word { x * 2 }",
    "loop main(t: Word) -> Word { let n = yield t + 1; n }",
    "data st { n: Word }\nloop main(x: Word) -> Word "
    "{ st.n = st.n + x; let r = yield st.n; r }",
    "struct P { x: Word, y: Word }\nfn main(x: Word) -> (Word, Text) "
    '{ let p = P { x: x, y: 2 }; (p.x + p.y, "hi") }',
    "enum S { A(Word), B }\nfn main(x: Word) -> Word "
    "{ match S::A(x) { S::A(v) => v, S::B => 0 } }",
    "fn main(x: Word) -> [Word; 4] { [x, x + 1, x + 2, x + 3] }",
    "use host::f(Word) -> Word\nfn main(x: Word) -> Word { host::f(x) + 1 }",
]
REFUSALS = (
    keleusma.KeleusmaError,
    TypeError,
    ValueError,
    OverflowError,
    MemoryError,
    KeyError,
)


def mutate(rng: random.Random, data: bytes) -> bytes:
    """Flip, set, delete or insert a few bytes, then usually repair the trailer."""
    out = bytearray(data)
    for _ in range(rng.randint(1, 4)):
        kind = rng.random()
        i = rng.randrange(len(out) - 4)
        if kind < 0.6:
            out[i] ^= 1 << rng.randrange(8)
        elif kind < 0.8:
            out[i] = rng.choice([0, 1, 0x7F, 0x80, 0xFF])
        elif kind < 0.9:
            del out[i]
        else:
            out.insert(i, rng.randrange(256))
    if rng.random() < 0.9 and len(out) > 8:
        out[-4:] = zlib.crc32(bytes(out[:-4])).to_bytes(4, "little")
    return bytes(out)


def identity(x: object) -> object:
    return x


def run_case(seed: int, bases: list[bytes]) -> str:
    rng = random.Random(seed)  # noqa: S311, reproducible fuzzing, not cryptography.
    data = mutate(rng, rng.choice(bases))
    try:
        module = keleusma.load_module(data)
        natives = {"host::f": identity} if b"host::f" in data else None
        vm = keleusma.VM(module, natives=natives)
        state = vm.call(3)
        for _ in range(3):
            if isinstance(state, keleusma.Finished):
                break
            state = vm.resume(1)
    except REFUSALS as error:
        return type(error).__name__
    return "ran"


def worker(start: int, count: int) -> None:
    bases = [keleusma.compile(s).to_bytes() for s in SOURCES]
    outcomes: collections.Counter[str] = collections.Counter()
    for seed in range(start, start + count):
        print(seed, file=sys.stderr, flush=True)
        outcomes[run_case(seed, bases)] += 1
    print(dict(outcomes))


def driver(batches: int, batch: int) -> int:
    totals: collections.Counter[str] = collections.Counter()
    crashes: list[tuple[int, int]] = []
    start, end = 0, batches * batch
    while start < end:
        count = min(batch, end - start)
        child = subprocess.run(  # noqa: S603, the interpreter and script are this file.
            [sys.executable, __file__, "--worker", str(start), str(count)],
            capture_output=True,
            text=True,
            timeout=3600,
            check=False,
        )
        if child.returncode != 0:
            seeds = [line for line in child.stderr.splitlines() if line.isdigit()]
            seed = int(seeds[-1]) if seeds else start
            crashes.append((seed, child.returncode))
            start = seed + 1
            continue
        totals.update(eval(child.stdout.strip().splitlines()[-1]))  # noqa: S307, our own output.
        start += count
    print("outcomes", dict(totals))
    print("crashes (seed, exit code)", crashes)
    return 1 if crashes else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--batches", type=int, default=20)
    parser.add_argument("--batch", type=int, default=1000)
    parser.add_argument("--worker", nargs=2, type=int, metavar=("START", "COUNT"))
    args = parser.parse_args()
    if args.worker:
        worker(*args.worker)
        return 0
    return driver(args.batches, args.batch)


if __name__ == "__main__":
    raise SystemExit(main())
