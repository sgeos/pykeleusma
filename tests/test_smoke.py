"""The seed's smoke tests. They prove the extension builds, links and imports."""

import tomllib
from pathlib import Path

import keleusma

ROOT = Path(__file__).resolve().parent.parent


def test_the_extension_imports_and_reports_versions() -> None:
    assert isinstance(keleusma.__version__, str)
    assert keleusma.__version__
    assert isinstance(keleusma.BYTECODE_VERSION, int)


def test_the_version_is_the_keleusma_release_and_a_binding_revision() -> None:
    # Both sides are derived. The version is read from the installed distribution
    # and from pyproject.toml, and must extend the Keleusma version in the wheel.
    project = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))
    assert keleusma.__version__ == project["project"]["version"]
    release, _, revision = keleusma.__version__.rpartition(".")
    assert release == keleusma.KELEUSMA_VERSION
    assert revision.isdigit()


def test_cargo_carries_the_first_three_parts_of_the_version() -> None:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    assert manifest["package"]["version"] == keleusma.KELEUSMA_VERSION


def test_the_stated_keleusma_version_matches_the_cargo_pin() -> None:
    # Both sides are derived. The pin is read from Cargo.toml and the stated value
    # from the built extension, so an upgrade that changes only one fails here.
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    dependency: object = manifest["dependencies"]["keleusma"]
    if isinstance(dependency, dict):
        pin: object = dependency["version"]  # pyright: ignore[reportUnknownVariableType]
    else:
        pin = dependency
    assert pin == "=" + keleusma.KELEUSMA_VERSION


def test_keleusma_0_2_2_reads_bytecode_version_1() -> None:
    # Keleusma 0.2.3 moves to version 2 and rejects version 1 artifacts. A failure
    # here after an upgrade is expected and must be handled, not silenced.
    assert keleusma.KELEUSMA_VERSION == "0.2.2"
    assert keleusma.BYTECODE_VERSION == 1
