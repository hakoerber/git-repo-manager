#!/usr/bin/env python3

from helpers import *

import git


def test_worktree_add_simple():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        files = os.listdir(base_dir)
        assert len(files) == 2
        assert set(files) == {".git-main-working-tree", "test"}

        repo = git.Repo(os.path.join(base_dir, "test"))
        assert not repo.bare
        assert not repo.is_dirty()
        assert str(repo.active_branch) == "test"
        assert repo.active_branch.tracking_branch() is None


def test_worktree_add_with_tracking():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        print(cmd.stderr)
        assert cmd.returncode == 0

        files = os.listdir(base_dir)
        assert len(files) == 2
        assert set(files) == {".git-main-working-tree", "test"}

        repo = git.Repo(os.path.join(base_dir, "test"))
        assert not repo.bare
        assert not repo.is_dirty()
        assert str(repo.active_branch) == "test"
        assert str(repo.active_branch.tracking_branch()) == "origin/test"


def test_worktree_delete():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" not in os.listdir(base_dir)

        cmd = grm(["wt", "add", "check"], cwd=base_dir)
        assert cmd.returncode == 0
        repo = git.Repo(os.path.join(base_dir, ".git-main-working-tree"))
        print(repo.branches)
        assert "test" not in [str(b) for b in repo.branches]


def test_worktree_delete_refusal_no_tracking_branch():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_refusal_uncommited_changes_new_file():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(f"cd {base_dir}/test && touch changed_file")

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_refusal_uncommited_changes_changed_file():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(f"cd {base_dir}/test && git ls-files | shuf | head | xargs rm -rf")

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_refusal_uncommited_changes_deleted_file():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"cd {base_dir}/test && git ls-files | shuf | head | while read f ; do echo $RANDOM > $f ; done"
        )

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_refusal_commited_changes():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f'cd {base_dir}/test && touch changed_file && git add changed_file && git commit -m "commitmsg"'
        )

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_refusal_tracking_branch_mismatch():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"cd {base_dir}/test && git push origin test && git reset --hard origin/test^"
        )

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_force_refusal():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        cmd = grm(["wt", "delete", "test", "--force"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" not in os.listdir(base_dir)


def test_worktree_add_delete_add():
    with TempGitRepositoryWorktree() as base_dir:
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" not in os.listdir(base_dir)

        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)
