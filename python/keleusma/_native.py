"""A Python callable offered to a script as a Keleusma native, with its bounds.

**The guarantee boundary.** Keleusma's worst-case bounds cover the script and the
bounds declared for each native. `wcet` and `wcmu_bytes` are attestations the host
makes and Keleusma trusts. A Python callable cannot honour them, because Python has no
bound on its own time or memory, so a script that calls a Python native has a worst
case only for its own part. Declaring the bounds still matters, because Keleusma sizes
the arena and computes `vm.wcet_per_iteration` from them.

Pass natives when building the VM, `keleusma.VM(module, natives={"host::f": f})`, so the
arena is sized with their memory bounds. A value is either a bare callable, which takes
Keleusma's default bounds, or a `Native` with explicit ones.
"""

from collections.abc import Callable
from dataclasses import dataclass

#: Keleusma's default worst-case time for a native call, in nominal cycles.
DEFAULT_NATIVE_WCET = 10
#: Keleusma's default worst-case arena use for a native call, in bytes.
DEFAULT_NATIVE_WCMU_BYTES = 0


@dataclass(frozen=True, slots=True)
class Native:
    """A native's callable together with the bounds declared for it.

    `wcet` is the worst-case time of one call in nominal cycles, and
    `wcmu_bytes` the worst-case arena memory one call uses, which includes a composite
    value the native returns, since Keleusma copies that into the arena. `None` takes
    Keleusma's default for that bound.
    """

    function: Callable[..., object]
    wcet: int | None = None
    wcmu_bytes: int | None = None
