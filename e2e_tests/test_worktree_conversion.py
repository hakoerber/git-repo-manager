#!/usr/bin/env python3

import tempfile

from helpers import *


def test_convert():
    with TempGitRepository() as git_dir:
        cmd = grm(["wt", "convert"], cwd=git_dir)
        assert cmd.returncode == 0

        files = os.listdir(git_dir)
        assert len(files) == 1
        assert files[0] == ".git-main-working-tree"

        cmd = grm(["wt", "add", "test"], cwd=git_dir)
        assert cmd.returncode == 0

        files = os.listdir(git_dir)
        assert len(files) == 2
        assert set(files) == {".git-main-working-tree", "test"}


def test_convert_already_worktree():
    with TempGitRepositoryWorktree() as (git_dir, _commit):
        before = checksum_directory(git_dir)

        cmd = grm(["wt", "convert"], cwd=git_dir)
        assert cmd.returncode != 0

        after = checksum_directory(git_dir)
        assert before == after


def test_convert_non_git():
    with NonGitDir() as dir:
        before = checksum_directory(dir)

        cmd = grm(["wt", "convert"], cwd=dir)
        assert cmd.returncode != 0

        after = checksum_directory(dir)
        assert before == after


def test_convert_empty():
    with EmptyDir() as dir:
        before = checksum_directory(dir)

        cmd = grm(["wt", "convert"], cwd=dir)
        assert cmd.returncode != 0

        after = checksum_directory(dir)
        assert before == after
