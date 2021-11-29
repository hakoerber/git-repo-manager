#!/usr/bin/env python3

from helpers import *


def test_worktree_clean():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" not in os.listdir(base_dir)


def test_worktree_clean_refusal_no_tracking_branch():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_clean_refusal_uncommited_changes_new_file():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(f"cd {base_dir}/test && touch changed_file")

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_clean_refusal_uncommited_changes_changed_file():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(f"cd {base_dir}/test && git ls-files | shuf | head | xargs rm -rf")

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_clean_refusal_uncommited_changes_cleand_file():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"cd {base_dir}/test && git ls-files | shuf | head | while read f ; do echo $RANDOM > $f ; done"
        )

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_clean_refusal_commited_changes():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f'cd {base_dir}/test && touch changed_file && git add changed_file && git commit -m "commitmsg"'
        )

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_clean_refusal_tracking_branch_mismatch():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"cd {base_dir}/test && git push origin test && git reset --hard origin/test^"
        )

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_clean_fail_from_subdir():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        cmd = grm(["wt", "clean"], cwd=f"{base_dir}/test")
        assert cmd.returncode != 0
        assert len(cmd.stdout) == 0
        assert len(cmd.stderr) != 0


def test_worktree_clean_non_worktree():
    with TempGitRepository() as git_dir:
        cmd = grm(["wt", "clean"], cwd=git_dir)
        assert cmd.returncode != 0
        assert len(cmd.stdout) == 0
        assert len(cmd.stderr) != 0


def test_worktree_clean_non_git():
    with NonGitDir() as base_dir:
        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode != 0
        assert len(cmd.stdout) == 0
        assert len(cmd.stderr) != 0
