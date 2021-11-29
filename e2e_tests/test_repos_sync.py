#!/usr/bin/env python3

import tempfile
import re

import pytest
import toml
import git

from helpers import *


def test_repos_sync_config_is_valid_symlink():
    with tempfile.TemporaryDirectory() as target:
        with TempGitFileRemote() as (remote, head_commit_sha):
            with tempfile.NamedTemporaryFile() as config:
                with tempfile.TemporaryDirectory() as config_dir:
                    config_symlink = os.path.join(config_dir, "cfglink")
                    os.symlink(config.name, config_symlink)

                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config_symlink])
                    assert cmd.returncode == 0

                    git_dir = os.path.join(target, "test")
                    assert os.path.exists(git_dir)
                    with git.Repo(git_dir) as repo:
                        assert not repo.bare
                        assert not repo.is_dirty()
                        assert set([str(r) for r in repo.remotes]) == {"origin"}
                        assert str(repo.active_branch) == "master"
                        assert str(repo.head.commit) == head_commit_sha


def test_repos_sync_config_is_invalid_symlink():
    with tempfile.TemporaryDirectory() as target:
        with tempfile.TemporaryDirectory() as config_dir:
            with NonExistentPath() as nonexistent_dir:
                config_symlink = os.path.join(config_dir, "cfglink")
                os.symlink(nonexistent_dir, config_symlink)

                cmd = grm(["repos", "sync", "--config", config_symlink])

                assert cmd.returncode != 0
                assert len(cmd.stdout) == 0
                assert "not found" in cmd.stderr.lower()
                assert not os.path.exists(os.path.join(target, "test"))
                assert not os.path.exists(os.path.join(target, "test"))


def test_repos_sync_config_is_directory():
    with tempfile.TemporaryDirectory() as config:
        cmd = grm(["repos", "sync", "--config", config])

        assert cmd.returncode != 0
        assert len(cmd.stdout) == 0
        assert "is a directory" in cmd.stderr.lower()


def test_repos_sync_config_is_unreadable():
    with tempfile.TemporaryDirectory() as config_dir:
        config_path = os.path.join(config_dir, "cfg")
        open(config_path, "w")
        os.chmod(config_path, 0o0000)
        cmd = grm(["repos", "sync", "--config", config_path])

        assert os.path.exists(config_path)
        assert cmd.returncode != 0
        assert len(cmd.stdout) == 0
        assert "permission denied" in cmd.stderr.lower()


def test_repos_sync_unmanaged_repos():
    with tempfile.TemporaryDirectory() as root:
        with TempGitRepository(dir=root) as unmanaged_repo:
            with tempfile.NamedTemporaryFile() as config:
                with open(config.name, "w") as f:
                    f.write(
                        f"""
                        [[trees]]
                        root = "{root}"

                        [[trees.repos]]
                        name = "test"
                    """
                    )

                cmd = grm(["repos", "sync", "--config", config.name])
                assert cmd.returncode == 0

                git_dir = os.path.join(root, "test")
                assert os.path.exists(git_dir)

                # this removes the prefix (root) from the path (unmanaged_repo)
                unmanaged_repo_name = os.path.relpath(unmanaged_repo, root)
                regex = f".*unmanaged.*{unmanaged_repo_name}"
                assert any([re.match(regex, l) for l in cmd.stderr.lower().split("\n")])


def test_repos_sync_root_is_file():
    with tempfile.NamedTemporaryFile() as target:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                f.write(
                    f"""
                    [[trees]]
                    root = "{target.name}"

                    [[trees.repos]]
                    name = "test"
                """
                )

            cmd = grm(["repos", "sync", "--config", config.name])
            assert cmd.returncode != 0
            assert len(cmd.stdout) == 0
            assert "not a directory" in cmd.stderr.lower()


def test_repos_sync_normal_clone():
    with tempfile.TemporaryDirectory() as target:
        with TempGitFileRemote() as (remote1, remote1_head_commit_sha):
            with TempGitFileRemote() as (remote2, remote2_head_commit_sha):
                with tempfile.NamedTemporaryFile() as config:
                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote1}"
                            type = "file"

                            [[trees.repos.remotes]]
                            name = "origin2"
                            url = "file://{remote2}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0

                    git_dir = os.path.join(target, "test")
                    assert os.path.exists(git_dir)
                    with git.Repo(git_dir) as repo:
                        assert not repo.bare
                        assert not repo.is_dirty()
                        assert set([str(r) for r in repo.remotes]) == {
                            "origin",
                            "origin2",
                        }
                        assert str(repo.active_branch) == "master"
                        assert str(repo.head.commit) == remote1_head_commit_sha

                        assert len(repo.remotes) == 2
                        urls = list(repo.remote("origin").urls)
                        assert len(urls) == 1
                        assert urls[0] == f"file://{remote1}"

                        urls = list(repo.remote("origin2").urls)
                        assert len(urls) == 1
                        assert urls[0] == f"file://{remote2}"


def test_repos_sync_normal_init():
    with tempfile.TemporaryDirectory() as target:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                f.write(
                    f"""
                    [[trees]]
                    root = "{target}"

                    [[trees.repos]]
                    name = "test"
                """
                )

            cmd = grm(["repos", "sync", "--config", config.name])
            assert cmd.returncode == 0

            git_dir = os.path.join(target, "test")
            assert os.path.exists(git_dir)
            with git.Repo(git_dir) as repo:
                assert not repo.bare
                assert not repo.is_dirty()
                # as there are no commits yet, HEAD does not point to anything
                # valid
                assert not repo.head.is_valid()


def test_repos_sync_normal_add_remote():
    with tempfile.TemporaryDirectory() as target:
        with TempGitFileRemote() as (remote1, remote1_head_commit_sha):
            with TempGitFileRemote() as (remote2, remote2_head_commit_sha):
                with tempfile.NamedTemporaryFile() as config:
                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote1}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0

                    git_dir = os.path.join(target, "test")

                    assert os.path.exists(git_dir)
                    with git.Repo(git_dir) as repo:
                        assert not repo.bare
                        assert not repo.is_dirty()
                        assert set([str(r) for r in repo.remotes]) == {"origin"}
                        assert str(repo.active_branch) == "master"
                        assert str(repo.head.commit) == remote1_head_commit_sha

                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote1}"
                            type = "file"

                            [[trees.repos.remotes]]
                            name = "origin2"
                            url = "file://{remote2}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0
                    with git.Repo(git_dir) as repo:
                        assert set([str(r) for r in repo.remotes]) == {
                            "origin",
                            "origin2",
                        }

                        urls = list(repo.remote("origin").urls)
                        assert len(urls) == 1
                        assert urls[0] == f"file://{remote1}"

                        urls = list(repo.remote("origin2").urls)
                        assert len(urls) == 1
                        assert urls[0] == f"file://{remote2}"


def test_repos_sync_normal_remove_remote():
    with tempfile.TemporaryDirectory() as target:
        with TempGitFileRemote() as (remote1, remote1_head_commit_sha):
            with TempGitFileRemote() as (remote2, remote2_head_commit_sha):
                with tempfile.NamedTemporaryFile() as config:
                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote1}"
                            type = "file"

                            [[trees.repos.remotes]]
                            name = "origin2"
                            url = "file://{remote2}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0

                    git_dir = os.path.join(target, "test")

                    assert os.path.exists(git_dir)
                    with git.Repo(git_dir) as repo:
                        assert not repo.bare
                        assert not repo.is_dirty()
                        assert set([str(r) for r in repo.remotes]) == {
                            "origin",
                            "origin2",
                        }
                        assert str(repo.active_branch) == "master"
                        assert str(repo.head.commit) == remote1_head_commit_sha

                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin2"
                            url = "file://{remote2}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0
                    shell(f"cd {git_dir} && git remote -v")
                    with git.Repo(git_dir) as repo:
                        """
                        There is some bug(?) in GitPython. It does not properly
                        detect removed remotes. It will still report the old
                        remove in repo.remotes.

                        So instead, we make sure that we get an Exception when
                        we try to access the old remove via repo.remote().

                        Note that repo.remote() checks the actual repo lazily.
                        Even `exists()` seems to just check against repo.remotes
                        and will return True even if the remote is not actually
                        configured. So we have to force GitPython to hit the filesystem.
                        calling Remotes.urls does. But it returns an iterator
                        that first has to be unwrapped via list(). Only THEN
                        do we actually get an exception of the remotes does not
                        exist.
                        """
                        with pytest.raises(git.exc.GitCommandError):
                            list(repo.remote("origin").urls)

                        urls = list(repo.remote("origin2").urls)
                        assert len(urls) == 1
                        assert urls[0] == f"file://{remote2}"


def test_repos_sync_normal_change_remote_url():
    with tempfile.TemporaryDirectory() as target:
        with TempGitFileRemote() as (remote1, remote1_head_commit_sha):
            with TempGitFileRemote() as (remote2, remote2_head_commit_sha):
                with tempfile.NamedTemporaryFile() as config:
                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote1}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0

                    git_dir = os.path.join(target, "test")

                    assert os.path.exists(git_dir)
                    with git.Repo(git_dir) as repo:
                        assert not repo.bare
                        assert not repo.is_dirty()
                        assert set([str(r) for r in repo.remotes]) == {"origin"}
                        assert str(repo.active_branch) == "master"
                        assert str(repo.head.commit) == remote1_head_commit_sha

                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote2}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0
                    with git.Repo(git_dir) as repo:
                        assert set([str(r) for r in repo.remotes]) == {"origin"}

                        urls = list(repo.remote("origin").urls)
                        assert len(urls) == 1
                        assert urls[0] == f"file://{remote2}"


def test_repos_sync_normal_change_remote_name():
    with tempfile.TemporaryDirectory() as target:
        with TempGitFileRemote() as (remote1, remote1_head_commit_sha):
            with TempGitFileRemote() as (remote2, remote2_head_commit_sha):
                with tempfile.NamedTemporaryFile() as config:
                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote1}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0

                    git_dir = os.path.join(target, "test")

                    assert os.path.exists(git_dir)
                    with git.Repo(git_dir) as repo:
                        assert not repo.bare
                        assert not repo.is_dirty()
                        assert set([str(r) for r in repo.remotes]) == {"origin"}
                        assert str(repo.active_branch) == "master"
                        assert str(repo.head.commit) == remote1_head_commit_sha

                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin2"
                            url = "file://{remote1}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0
                    with git.Repo(git_dir) as repo:
                        # See the note in `test_repos_sync_normal_remove_remote()`
                        # about repo.remotes
                        with pytest.raises(git.exc.GitCommandError):
                            list(repo.remote("origin").urls)

                        urls = list(repo.remote("origin2").urls)
                        assert len(urls) == 1
                        assert urls[0] == f"file://{remote1}"


def test_repos_sync_worktree_clone():
    with tempfile.TemporaryDirectory() as target:
        with TempGitFileRemote() as (remote, head_commit_sha):
            with tempfile.NamedTemporaryFile() as config:
                with open(config.name, "w") as f:
                    f.write(
                        f"""
                        [[trees]]
                        root = "{target}"

                        [[trees.repos]]
                        name = "test"
                        worktree_setup = true

                        [[trees.repos.remotes]]
                        name = "origin"
                        url = "file://{remote}"
                        type = "file"
                    """
                    )

                cmd = grm(["repos", "sync", "--config", config.name])
                assert cmd.returncode == 0

                worktree_dir = f"{target}/test"
                assert os.path.exists(worktree_dir)

                assert set(os.listdir(worktree_dir)) == {".git-main-working-tree"}

                with git.Repo(
                    os.path.join(worktree_dir, ".git-main-working-tree")
                ) as repo:
                    assert repo.bare
                    assert set([str(r) for r in repo.remotes]) == {"origin"}
                    assert str(repo.active_branch) == "master"
                    assert str(repo.head.commit) == head_commit_sha


def test_repos_sync_worktree_init():
    with tempfile.TemporaryDirectory() as target:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                f.write(
                    f"""
                    [[trees]]
                    root = "{target}"

                    [[trees.repos]]
                    name = "test"
                    worktree_setup = true
                """
                )

            cmd = grm(["repos", "sync", "--config", config.name])
            assert cmd.returncode == 0

            worktree_dir = f"{target}/test"
            assert os.path.exists(worktree_dir)

            assert set(os.listdir(worktree_dir)) == {".git-main-working-tree"}
            with git.Repo(os.path.join(worktree_dir, ".git-main-working-tree")) as repo:
                assert repo.bare
                # as there are no commits yet, HEAD does not point to anything
                # valid
                assert not repo.head.is_valid()


def test_repos_sync_invalid_toml():
    with tempfile.NamedTemporaryFile() as config:
        with open(config.name, "w") as f:
            f.write(
                f"""
                [[trees]]
                root = invalid as there are no quotes ;)
            """
            )
            cmd = grm(["repos", "sync", "--config", config.name])
            assert cmd.returncode != 0


def test_repos_sync_unchanged():
    with tempfile.TemporaryDirectory() as target:
        with TempGitFileRemote() as (remote1, remote1_head_commit_sha):
            with TempGitFileRemote() as (remote2, remote2_head_commit_sha):
                with tempfile.NamedTemporaryFile() as config:
                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote1}"
                            type = "file"

                            [[trees.repos.remotes]]
                            name = "origin2"
                            url = "file://{remote2}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0

                    before = checksum_directory(target)
                    cmd = grm(["repos", "sync", "--config", config.name])
                    after = checksum_directory(target)
                    assert cmd.returncode == 0

                    assert before == after


def test_repos_sync_normal_change_to_worktree():
    with tempfile.TemporaryDirectory() as target:
        with TempGitFileRemote() as (remote1, remote1_head_commit_sha):
            with TempGitFileRemote() as (remote2, remote2_head_commit_sha):
                with tempfile.NamedTemporaryFile() as config:
                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote1}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0

                    git_dir = os.path.join(target, "test")

                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"
                            worktree_setup = true

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote1}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode != 0
                    assert "already exists" in cmd.stderr
                    assert "not using a worktree setup" in cmd.stderr


def test_repos_sync_worktree_change_to_normal():
    with tempfile.TemporaryDirectory() as target:
        with TempGitFileRemote() as (remote1, remote1_head_commit_sha):
            with TempGitFileRemote() as (remote2, remote2_head_commit_sha):
                with tempfile.NamedTemporaryFile() as config:
                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"
                            worktree_setup = true

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote1}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode == 0

                    git_dir = os.path.join(target, "test")

                    with open(config.name, "w") as f:
                        f.write(
                            f"""
                            [[trees]]
                            root = "{target}"

                            [[trees.repos]]
                            name = "test"

                            [[trees.repos.remotes]]
                            name = "origin"
                            url = "file://{remote1}"
                            type = "file"
                        """
                        )

                    cmd = grm(["repos", "sync", "--config", config.name])
                    assert cmd.returncode != 0
                    assert "already exists" in cmd.stderr
                    assert "using a worktree setup" in cmd.stderr
