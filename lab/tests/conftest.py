"""Shared test setup: every test's git calls target the temporary repository it made.

Git reads `GIT_DIR`, `GIT_WORK_TREE` and their kin before `-C` or `cwd`, so a test run in a
container that sets them (to make git work in a mounted worktree) would otherwise init, add and
commit in the mounted checkout. The tests' own helpers drop them too (`clean_git_env`).
"""

from __future__ import annotations

import os

import pytest

#: Variables that point git at a repository other than the one under `cwd` or `-C`.
GIT_REDIRECTS = (
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
    "GIT_NAMESPACE",
)


def clean_git_env() -> dict[str, str]:
    """The environment without any variable that redirects git."""
    return {k: v for k, v in os.environ.items() if k not in GIT_REDIRECTS}


@pytest.fixture(autouse=True)
def isolated_git(monkeypatch: pytest.MonkeyPatch) -> None:
    for name in GIT_REDIRECTS:
        monkeypatch.delenv(name, raising=False)
