"""git-cliff positive selection: every PR counts once, stray branch commits never leak.

A PR lands on ``main`` as one commit whose subject GitHub ends with the PR number,
``(#N)`` (``squash_merge_commit_title=PR_TITLE``). ``cliff.toml`` selects PR commits by
requiring that suffix in every type parser, so an ordinary commit inside a PR branch —
which has no ``(#N)`` — is dropped instead of counting a PR a second time. Merges before
2026-11-01 read "Merge pull request #N from <branch>" with the PR title in the body, and
the preprocessor lifts that title to the first line so the old history is counted too.
"""

import os
import shutil
import subprocess
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
CLIFF = ROOT / "cliff.toml"
GIT_CLIFF = "git-cliff==2.14.2"

pytestmark = pytest.mark.skipif(
    shutil.which("git") is None or shutil.which("uv") is None,
    reason="git and uv are required to drive git-cliff",
)


def _env(base: dict[str, str], stamp: datetime) -> dict[str, str]:
    iso = stamp.strftime("%Y-%m-%dT%H:%M:%S +0000")
    env = dict(base)
    env["GIT_AUTHOR_DATE"] = iso
    env["GIT_COMMITTER_DATE"] = iso
    return env


def _git(repo: Path, base: dict[str, str], stamp: datetime, *args: str) -> str:
    run = subprocess.run(
        ["git", "-c", "commit.gpgsign=false", *args],
        cwd=repo,
        env=_env(base, stamp),
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=True,
    )
    return run.stdout


def _cliff(repo: Path, *args: str) -> str:
    run = subprocess.run(
        ["uv", "run", "--no-project", "--with", GIT_CLIFF, "git-cliff", *args],
        cwd=repo,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=True,
    )
    return run.stdout


@pytest.fixture()
def history(tmp_path: Path) -> Path:
    repo = tmp_path / "repo"
    repo.mkdir()
    base = dict(os.environ)
    t0 = datetime(2026, 1, 1, tzinfo=UTC)
    step = timedelta(minutes=1)

    _git(repo, base, t0, "init", "--initial-branch=main")
    # Configure an identity so the test never depends on the developer's global config.
    _git(repo, base, t0, "config", "user.name", "Test")
    _git(repo, base, t0, "config", "user.email", "test@example.com")

    # The last release: the range under test is everything committed after this tag.
    _git(repo, base, t0, "commit", "--allow-empty", "-m", "chore: initial")
    _git(repo, base, t0, "tag", "v0.1.0")

    commits = [
        # Old merge: GitHub pre-squash wrote this subject with the PR title in the body.
        ("Merge pull request #10 from old/branch", "feat: legacy merged feature"),
        ("feat(game): add map parser (#11)", None),
        ("fix: correct rle run (#12)", None),
        ("docs: update notes (#13)", None),  # service type: excluded by its parser
        ("feat: excluded without suffix", None),  # a stray branch commit: no (#N)
        ("feat: counted by suffix (#14)", None),  # same shape as the stray, with (#N)
    ]
    for i, (subject, body) in enumerate(commits, start=1):
        args = ["commit", "--allow-empty", "-m", subject]
        if body is not None:
            args += ["-m", body]
        _git(repo, base, t0 + i * step, *args)

    shutil.copyfile(CLIFF, repo / "cliff.toml")
    return repo


def test_only_pr_commits_reach_the_changelog(history: Path) -> None:
    out = _cliff(history, "--unreleased", "--strip", "all", "--config", "cliff.toml").lower()

    # Every PR counted exactly once: old merge, squash feat, squash fix.
    for needle in (
        "legacy merged feature",
        "add map parser",
        "correct rle run",
        "counted by suffix",
    ):
        assert out.count(needle) == 1, f"expected '{needle}' exactly once in:\n{out}"

    # A service type is dropped even though it carries (#N).
    assert "update notes" not in out
    # The stray branch commit shares the feat shape but has no (#N): positive selection drops it.
    assert "excluded without suffix" not in out


def test_feature_since_tag_bumps_minor(history: Path) -> None:
    out = _cliff(history, "--bumped-version", "--config", "cliff.toml").strip()
    assert out == "v0.2.0"
