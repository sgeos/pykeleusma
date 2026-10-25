"""The example scripts that ship with Keleusma 0.2.2, run through pykeleusma.

Each script states its result in a comment of the form `// Expected output: N`. The
expected value is read from the script itself, so the test pins nothing of its own, and
a script without that comment fails rather than passing silently.
"""

import re
from pathlib import Path

import pytest

import keleusma

SCRIPTS = Path(__file__).resolve().parent / "keleusma_scripts"
EXPECTED = re.compile(r"//\s*Expected output:\s*(\S+)")


def _scripts() -> list[Path]:
    return sorted(SCRIPTS.glob("*.kel"))


def test_every_vendored_script_is_collected() -> None:
    # Ten scripts were copied from the crate. A glob that matched nothing would make
    # the parametrised test below collect zero cases and pass.
    assert len(_scripts()) == 10


@pytest.mark.parametrize("script", _scripts(), ids=lambda p: p.stem)
def test_a_crate_script_gives_its_stated_output(script: Path) -> None:
    source = script.read_text(encoding="utf-8")
    match = EXPECTED.search(source)
    assert match is not None, f"{script.name} states no expected output"
    state = keleusma.VM(keleusma.compile(source)).call()
    assert isinstance(state, keleusma.Finished)
    assert str(state.value) == match.group(1)


def test_information_flow_labels_vanish_at_the_boundary() -> None:
    # The book's chapter 24. A label exists only while the program is checked, so a
    # labelled value converts as its underlying type in both directions.
    out = keleusma.compile("fn main(x: Word) -> Word@Master { classify x@Master }")
    assert keleusma.VM(out).call(4) == keleusma.Finished(4)
    back = keleusma.compile("fn main(x: Word@Master) -> Word { declassify x@Master }")
    assert keleusma.VM(back).call(4) == keleusma.Finished(4)
