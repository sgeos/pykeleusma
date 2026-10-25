"""What the distributions carry, checked before publication.

The wheel links Rust crates whose licenses require their notices in binary copies, so
the notices file must match Cargo.lock and be listed for the wheel. The check runs the
generator itself, so it needs cargo and the crates' sources, which the build fetched.
"""

import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROJECT = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))


def test_the_third_party_notices_match_the_lock_file() -> None:
    result = subprocess.run(  # noqa: S603
        [sys.executable, str(ROOT / "tools" / "third_party_licenses.py"), "--check"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr


def test_the_wheel_lists_every_license_file() -> None:
    files = PROJECT["project"]["license-files"]
    assert files == ["LICENSE", "THIRD_PARTY_LICENSES.txt"]
    for name in files:
        assert (ROOT / name).is_file()


def test_the_project_links_point_at_this_repository() -> None:
    urls: dict[str, str] = PROJECT["project"]["urls"]
    assert urls["Repository"] == "https://github.com/sgeos/pykeleusma"
    assert all(url.startswith("https://") for url in urls.values())


def test_the_readme_has_no_relative_links() -> None:
    # PyPI renders the README without the repository around it, so a relative link
    # would lead nowhere on the project page.
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    targets = re.findall(r"\]\(([^)]+)\)", readme)
    assert targets
    assert [t for t in targets if not t.startswith(("https://", "#"))] == []


def test_every_classifier_is_one_pypi_accepts() -> None:
    from trove_classifiers import classifiers

    declared: list[str] = PROJECT["project"]["classifiers"]
    assert [c for c in declared if c not in classifiers] == []
