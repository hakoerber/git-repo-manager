#!/usr/bin/env python3

import re

from helpers import *

import pytest


@pytest.mark.parametrize("has_config", [True, False])
def test_worktree_status(has_config):
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        if has_config:
            with open(os.path.join(base_dir, "grm.toml"), "w") as f:
                f.write("")
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        cmd = grm(["wt", "status"], cwd=base_dir)
        assert cmd.returncode == 0
        assert len(cmd.stderr) == 0
        stdout = cmd.stdout.lower()
        assert "test" in stdout


def test_worktree_status_fail_from_subdir():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        cmd = grm(["wt", "status"], cwd=f"{base_dir}/test")
        assert cmd.returncode != 0
        assert len(cmd.stdout) == 0
        assert len(cmd.stderr) != 0


def test_worktree_status_non_worktree():
    with TempGitRepository() as git_dir:
        cmd = grm(["wt", "status"], cwd=git_dir)
        assert cmd.returncode != 0
        assert len(cmd.stdout) == 0
        assert len(cmd.stderr) != 0


def test_worktree_status_non_git():
    with NonGitDir() as base_dir:
        cmd = grm(["wt", "status"], cwd=base_dir)
        assert cmd.returncode != 0
        assert len(cmd.stdout) == 0
        assert len(cmd.stderr) != 0


def test_worktree_status_warn_with_non_worktree_dir():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"""
            cd {base_dir}
            mkdir not_a_worktree
            """
        )

        cmd = grm(["wt", "status"], cwd=base_dir)

        assert cmd.returncode == 0
        assert len(cmd.stdout) != 0
        assert len(cmd.stderr) != 0
        assert (
            re.match(
                ".*error.*not_a_worktree.*not a valid worktree directory",
                cmd.stderr,
                re.IGNORECASE,
            )
            is not None
        )
