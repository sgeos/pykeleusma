"""Roadmap task 9. The README states the guarantee boundary, and its example runs.

A statement about guarantees that drifted from the code would mislead exactly the
reader who relies on it, so these tests pin the section's key claims and execute the
README's usage example.
"""

import re
from pathlib import Path

README = (Path(__file__).resolve().parent.parent / "README.md").read_text("utf-8")


def _section(title: str) -> str:
    match = re.search(rf"^## {re.escape(title)}\n(.*?)(?=^## )", README, re.S | re.M)
    assert match is not None, f"the README has no section {title!r}"
    return match.group(1)


def test_the_readme_states_the_guarantee_boundary() -> None:
    section = _section("What is guaranteed, and what is not")
    assert "the script, and the bounds declared for each native" in section
    assert "It never covers\nPython." in section
    assert "Python code is outside every bound." in section
    assert "promises, not limits" in section
    assert "not cycles\n  or seconds" in section


def test_the_readme_names_its_empirical_limits() -> None:
    section = _section("What is guaranteed, and what is not")
    assert "measured rather than proven" in section
    assert "cannot check every expression" in section


def test_the_readme_usage_example_runs() -> None:
    block = re.search(r"## Using it.*?```python\n(.*?)```", README, re.S)
    assert block is not None
    code = block.group(1)
    # The two bare resumes carry their results in comments, which become assertions.
    code = code.replace(
        "stream.resume(10)", "assert stream.resume(10) == keleusma.Reset()"
    ).replace("stream.resume(20)", "assert stream.resume(20) == keleusma.Yielded(40)")
    exec(compile(code, "README.md", "exec"), {})  # noqa: S102 - the repository's own README
