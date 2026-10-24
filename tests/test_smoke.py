"""The seed's smoke tests. They prove the extension builds, links and imports."""

import tomllib
from pathlib import Path

import keleusma

ROOT = Path(__file__).resolve().parent.parent


def test_the_extension_imports_and_reports_versions() -> None:
    assert isinstance(keleusma.__version__, str)
    assert keleusma.__version__
    assert isinstance(keleusma.BYTECODE_VERSION, int)


def test_the_stated_keleusma_version_matches_the_cargo_pin() -> None:
    # Both sides are derived. The pin is read from Cargo.toml and the stated value
    # from the built extension, so an upgrade that changes only one fails here.
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    pin = manifest["dependencies"]["keleusma"]
    assert pin == "=" + keleusma.KELEUSMA_VERSION


def test_keleusma_0_2_2_reads_bytecode_version_1() -> None:
    # Keleusma 0.2.3 moves to version 2 and rejects version 1 artifacts. A failure
    # here after an upgrade is expected and must be handled, not silenced.
    assert keleusma.KELEUSMA_VERSION == "0.2.2"
    assert keleusma.BYTECODE_VERSION == 1
