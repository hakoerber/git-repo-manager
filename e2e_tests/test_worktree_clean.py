#!/usr/bin/env python3

import pytest

from helpers import *


def test_worktree_clean():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" not in os.listdir(base_dir)


def test_worktree_clean_refusal_no_tracking_branch():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_clean_refusal_uncommited_changes_new_file():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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


@pytest.mark.parametrize("configure_default_branch", [True, False])
@pytest.mark.parametrize("branch_list_empty", [True, False])
def test_worktree_clean_configured_default_branch(
    configure_default_branch, branch_list_empty
):
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        if configure_default_branch:
            with open(os.path.join(base_dir, "grm.toml"), "w") as f:
                if branch_list_empty:
                    f.write(
                        f"""
                        persistent_branches = []
                    """
                    )
                else:
                    f.write(
                        f"""
                        persistent_branches = [
                            "mybranch"
                        ]
                    """
                    )

        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"""
            cd {base_dir}
            (
                cd ./test
                touch change
                git add change
                git commit -m commit
            )

            git --git-dir ./.git-main-working-tree worktree add mybranch
            (
                cd ./mybranch
                git merge --no-ff test
            )
            git --git-dir ./.git-main-working-tree worktree remove mybranch
        """
        )

        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0
        if configure_default_branch and not branch_list_empty:
            assert "test" not in os.listdir(base_dir)
        else:
            assert "test" in os.listdir(base_dir)
