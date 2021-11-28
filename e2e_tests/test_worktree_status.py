#!/usr/bin/env python3

from helpers import *


def test_worktree_status():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        cmd = grm(["wt", "status"], cwd=base_dir)
        assert cmd.returncode == 0
        assert len(cmd.stderr) == 0
        stdout = cmd.stdout.lower()
        assert "test" in stdout


def test_worktree_status_fail_from_subdir():
    with TempGitRepositoryWorktree() as base_dir:
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
