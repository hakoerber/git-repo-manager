#!/usr/bin/env python3

from helpers import *

import git
import pytest

import os.path


@pytest.mark.parametrize("remote_branch_already_exists", [True, False])
@pytest.mark.parametrize("has_config", [True, False])
@pytest.mark.parametrize("has_default", [True, False])
@pytest.mark.parametrize("has_prefix", [True, False])
@pytest.mark.parametrize("worktree_with_slash", [True, False])
def test_worktree_add(
    remote_branch_already_exists,
    has_config,
    has_default,
    has_prefix,
    worktree_with_slash,
):
    if worktree_with_slash:
        worktree_name = "dir/test"
    else:
        worktree_name = "test"
    with TempGitRepositoryWorktree() as (base_dir, _commit):
        if has_config:
            with open(os.path.join(base_dir, "grm.toml"), "w") as f:
                f.write(
                    f"""
                [track]
                default = {str(has_default).lower()}
                default_remote = "origin"
                """
                )
                if has_prefix:
                    f.write(
                        """
                    default_remote_prefix = "myprefix"
                    """
                    )

        if remote_branch_already_exists:
            shell(
                f"""
                cd {base_dir}
                git --git-dir ./.git-main-working-tree worktree add tmp
                (
                    cd tmp
                    touch change
                    git add change
                    git commit -m commit
                    git push origin HEAD:{worktree_name}
                    #git reset --hard 'HEAD@{1}'
                    git branch -va
                )
                git --git-dir ./.git-main-working-tree worktree remove tmp
            """
            )
        cmd = grm(["wt", "add", worktree_name], cwd=base_dir)
        assert cmd.returncode == 0

        files = os.listdir(base_dir)
        if has_config is True:
            assert len(files) == 3
            if worktree_with_slash:
                assert set(files) == {".git-main-working-tree", "grm.toml", "dir"}
                assert set(os.listdir(os.path.join(base_dir, "dir"))) == {"test"}
            else:
                assert set(files) == {".git-main-working-tree", "grm.toml", "test"}
        else:
            assert len(files) == 2
            if worktree_with_slash:
                assert set(files) == {".git-main-working-tree", "dir"}
                assert set(os.listdir(os.path.join(base_dir, "dir"))) == {"test"}
            else:
                assert set(files) == {".git-main-working-tree", "test"}

        repo = git.Repo(os.path.join(base_dir, worktree_name))
        assert not repo.bare
        assert not repo.is_dirty()
        if has_config and has_default:
            if has_prefix and not remote_branch_already_exists:
                assert (
                    str(repo.active_branch.tracking_branch())
                    == f"origin/myprefix/{worktree_name}"
                )
            else:
                assert (
                    str(repo.active_branch.tracking_branch())
                    == f"origin/{worktree_name}"
                )
        else:
            assert repo.active_branch.tracking_branch() is None


def test_worktree_add_invalid_name():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        for worktree_name in [
            "/absolute/path",
            "trailingslash/",
            "with spaces",
            "with\t tabs",
            "with\nnewline",
        ]:
            args = ["wt", "add", worktree_name]
            cmd = grm(args, cwd=base_dir)
            assert cmd.returncode != 0
            print(cmd.stdout)
            print(cmd.stderr)
            assert not os.path.exists(worktree_name)
            assert not os.path.exists(os.path.join(base_dir, worktree_name))
            assert "invalid worktree name" in str(cmd.stderr.lower())


def test_worktree_add_invalid_track():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        for track in ["/absolute/path", "trailingslash/", "/"]:
            args = ["wt", "add", "foo", "--track", track]
            cmd = grm(args, cwd=base_dir)
            assert cmd.returncode != 0
            assert len(cmd.stderr.strip().split("\n")) == 1
            assert not os.path.exists("foo")
            assert not os.path.exists(os.path.join(base_dir, "foo"))
            assert "tracking branch" in str(cmd.stderr.lower())


@pytest.mark.parametrize("remote_branch_already_exists", [True, False])
@pytest.mark.parametrize("has_config", [True, False])
@pytest.mark.parametrize("has_default", [True, False])
@pytest.mark.parametrize("has_prefix", [True, False])
def test_worktree_add_with_tracking(
    remote_branch_already_exists, has_config, has_default, has_prefix
):
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        if has_config:
            with open(os.path.join(base_dir, "grm.toml"), "w") as f:
                f.write(
                    f"""
                [track]
                default = {str(has_default).lower()}
                default_remote = "origin"
                """
                )
                if has_prefix:
                    f.write(
                        """
                    default_remote_prefix = "myprefix"
                    """
                    )

        if remote_branch_already_exists:
            shell(
                f"""
                cd {base_dir}
                git --git-dir ./.git-main-working-tree worktree add tmp
                (
                    cd tmp
                    touch change
                    git add change
                    git commit -m commit
                    git push origin HEAD:test
                    #git reset --hard 'HEAD@{1}'
                    git branch -va
                )
                git --git-dir ./.git-main-working-tree worktree remove tmp
            """
            )
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        print(cmd.stderr)
        assert cmd.returncode == 0

        files = os.listdir(base_dir)
        if has_config is True:
            assert len(files) == 3
            assert set(files) == {".git-main-working-tree", "grm.toml", "test"}
        else:
            assert len(files) == 2
            assert set(files) == {".git-main-working-tree", "test"}

        repo = git.Repo(os.path.join(base_dir, "test"))
        assert not repo.bare
        assert not repo.is_dirty()
        assert str(repo.active_branch) == "test"
        assert str(repo.active_branch.tracking_branch()) == "origin/test"


@pytest.mark.parametrize("has_config", [True, False])
@pytest.mark.parametrize("has_default", [True, False])
@pytest.mark.parametrize("has_prefix", [True, False])
@pytest.mark.parametrize("track", [True, False])
def test_worktree_add_with_explicit_no_tracking(
    has_config, has_default, has_prefix, track
):
    with TempGitRepositoryWorktree() as (base_dir, _commit):
        if has_config:
            with open(os.path.join(base_dir, "grm.toml"), "w") as f:
                f.write(
                    f"""
                [track]
                default = {str(has_default).lower()}
                default_remote = "origin"
                """
                )
                if has_prefix:
                    f.write(
                        """
                    default_remote_prefix = "myprefix"
                    """
                    )
        if track is True:
            cmd = grm(
                ["wt", "add", "test", "--track", "origin/test", "--no-track"],
                cwd=base_dir,
            )
        else:
            cmd = grm(["wt", "add", "test", "--no-track"], cwd=base_dir)
        print(cmd.stderr)
        assert cmd.returncode == 0

        files = os.listdir(base_dir)
        if has_config is True:
            assert len(files) == 3
            assert set(files) == {".git-main-working-tree", "grm.toml", "test"}
        else:
            assert len(files) == 2
            assert set(files) == {".git-main-working-tree", "test"}

        repo = git.Repo(os.path.join(base_dir, "test"))
        assert not repo.bare
        assert not repo.is_dirty()
        assert str(repo.active_branch) == "test"
        assert repo.active_branch.tracking_branch() is None
def test_worktree_add_into_invalid_subdirectory():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "/dir/test"], cwd=base_dir)
        assert cmd.returncode == 1
        assert "dir" not in os.listdir(base_dir)
        assert "dir" not in os.listdir("/")

        cmd = grm(["wt", "add", "dir/"], cwd=base_dir)
        assert cmd.returncode == 1
        assert "dir" not in os.listdir(base_dir)


def test_worktree_delete():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
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
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        cmd = grm(["wt", "delete", "test", "--force"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" not in os.listdir(base_dir)


def test_worktree_add_delete_add():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" not in os.listdir(base_dir)

        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)
