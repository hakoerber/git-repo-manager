#!/usr/bin/env python3

import os.path

import git
from helpers import TempGitRepositoryWorktree, checksum_directory, funcname, grm, shell


def test_worktree_never_clean_persistent_branches():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        with open(os.path.join(base_dir, "grm.toml"), "w") as f:
            f.write(
                """
            persistent_branches = [
                "mybranch",
            ]
            """
            )

        cmd = grm(["wt", "add", "mybranch", "--track", "origin/master"], cwd=base_dir)
        assert cmd.returncode == 0

        before = checksum_directory(f"{base_dir}/mybranch")

        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0

        assert "mybranch" in os.listdir(base_dir)
        repo = git.Repo(os.path.join(base_dir, "mybranch"))
        assert str(repo.active_branch) == "mybranch"

        after = checksum_directory(f"{base_dir}/mybranch")
        assert before == after


def test_worktree_clean_branch_merged_into_persistent():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        with open(os.path.join(base_dir, "grm.toml"), "w") as f:
            f.write(
                """
            persistent_branches = [
                "master",
            ]
            """
            )

        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"""
            cd {base_dir}/test
            touch change1
            git add change1
            git commit -m "commit1"
            """
        )

        cmd = grm(["wt", "add", "master"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"""
            cd {base_dir}/master
            git merge --no-ff test
            """
        )

        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0

        assert "test" not in os.listdir(base_dir)


def test_worktree_no_clean_unmerged_branch():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        with open(os.path.join(base_dir, "grm.toml"), "w") as f:
            f.write(
                """
            persistent_branches = [
                "master",
            ]
            """
            )

        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"""
            cd {base_dir}/test
            touch change1
            git add change1
            git commit -m "commit1"
            git push origin test
            """
        )

        cmd = grm(["wt", "add", "master"], cwd=base_dir)
        assert cmd.returncode == 0

        cmd = grm(["wt", "clean"], cwd=base_dir)
        assert cmd.returncode == 0

        assert "test" in os.listdir(base_dir)


def test_worktree_delete_branch_merged_into_persistent():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        with open(os.path.join(base_dir, "grm.toml"), "w") as f:
            f.write(
                """
            persistent_branches = [
                "master",
            ]
            """
            )

        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"""
            cd {base_dir}/test
            touch change1
            git add change1
            git commit -m "commit1"
            """
        )

        cmd = grm(["wt", "add", "master"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"""
            cd {base_dir}/master
            git merge --no-ff test
            """
        )

        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        assert "test" not in os.listdir(base_dir)
