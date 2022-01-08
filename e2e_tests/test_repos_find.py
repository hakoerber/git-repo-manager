#!/usr/bin/env python3

import tempfile

import toml

from helpers import *


def test_repos_find_nonexistent():
    with NonExistentPath() as nonexistent_dir:
        cmd = grm(["repos", "find", nonexistent_dir])
        assert "does not exist" in cmd.stderr.lower()
        assert cmd.returncode != 0
        assert not os.path.exists(nonexistent_dir)


def test_repos_find_file():
    with tempfile.NamedTemporaryFile() as tmpfile:
        cmd = grm(["repos", "find", tmpfile.name])
        assert "not a directory" in cmd.stderr.lower()
        assert cmd.returncode != 0


def test_repos_find_empty():
    with tempfile.TemporaryDirectory() as tmpdir:
        cmd = grm(["repos", "find", tmpdir])
        assert cmd.returncode == 0
        assert len(cmd.stdout) == 0
        assert len(cmd.stderr) != 0


def test_repos_find_non_git_repos():
    with tempfile.TemporaryDirectory() as tmpdir:
        shell(
            f"""
            cd {tmpdir}
            mkdir non_git
            (
                cd ./non_git
                echo test > test
            )
        """
        )

        cmd = grm(["repos", "find", tmpdir])

        assert cmd.returncode == 0
        assert len(cmd.stdout) == 0
        assert len(cmd.stderr) != 0


def test_repos_find():
    with tempfile.TemporaryDirectory() as tmpdir:
        shell(
            f"""
            cd {tmpdir}
            mkdir repo1
            (
                cd ./repo1
                git init
                echo test > test
                git add test
                git commit -m "commit1"
                git remote add origin https://example.com/repo2.git
                git remote add someremote ssh://example.com/repo2.git
            )
            mkdir repo2
            (
                cd ./repo2
                git init
                git co -b main
                echo test > test
                git add test
                git commit -m "commit1"
                git remote add origin https://example.com/repo2.git
            )
            mkdir non_git
            (
                cd non_git
                echo test > test
            )
        """
        )

        cmd = grm(["repos", "find", tmpdir])
        assert cmd.returncode == 0
        assert len(cmd.stderr) == 0

        output = toml.loads(cmd.stdout)

        assert isinstance(output, dict)
        assert set(output.keys()) == {"trees"}
        assert isinstance(output["trees"], list)
        assert len(output["trees"]) == 1
        for tree in output["trees"]:
            assert set(tree.keys()) == {"root", "repos"}
            assert tree["root"] == tmpdir
            assert isinstance(tree["repos"], list)
            assert len(tree["repos"]) == 2

            repo1 = [r for r in tree["repos"] if r["name"] == "repo1"][0]
            assert repo1["worktree_setup"] is False
            assert isinstance(repo1["remotes"], list)
            assert len(repo1["remotes"]) == 2

            origin = [r for r in repo1["remotes"] if r["name"] == "origin"][0]
            assert set(origin.keys()) == {"name", "type", "url"}
            assert origin["type"] == "https"
            assert origin["url"] == "https://example.com/repo2.git"

            someremote = [r for r in repo1["remotes"] if r["name"] == "someremote"][0]
            assert set(origin.keys()) == {"name", "type", "url"}
            assert someremote["type"] == "ssh"
            assert someremote["url"] == "ssh://example.com/repo2.git"

            repo2 = [r for r in tree["repos"] if r["name"] == "repo2"][0]
            assert repo2["worktree_setup"] is False
            assert isinstance(repo1["remotes"], list)
            assert len(repo2["remotes"]) == 1

            origin = [r for r in repo2["remotes"] if r["name"] == "origin"][0]
            assert set(origin.keys()) == {"name", "type", "url"}
            assert origin["type"] == "https"
            assert origin["url"] == "https://example.com/repo2.git"


def test_repos_find_in_root():
    with TempGitRepository() as repo_dir:

        cmd = grm(["repos", "find", repo_dir])
        assert cmd.returncode == 0
        assert len(cmd.stderr) == 0

        output = toml.loads(cmd.stdout)

        assert isinstance(output, dict)
        assert set(output.keys()) == {"trees"}
        assert isinstance(output["trees"], list)
        assert len(output["trees"]) == 1
        for tree in output["trees"]:
            assert set(tree.keys()) == {"root", "repos"}
            assert tree["root"] == os.path.dirname(repo_dir)
            assert isinstance(tree["repos"], list)
            assert len(tree["repos"]) == 1

            repo1 = [
                r for r in tree["repos"] if r["name"] == os.path.basename(repo_dir)
            ][0]
            assert repo1["worktree_setup"] is False
            assert isinstance(repo1["remotes"], list)
            assert len(repo1["remotes"]) == 2

            origin = [r for r in repo1["remotes"] if r["name"] == "origin"][0]
            assert set(origin.keys()) == {"name", "type", "url"}
            assert origin["type"] == "file"

            someremote = [r for r in repo1["remotes"] if r["name"] == "otherremote"][0]
            assert set(origin.keys()) == {"name", "type", "url"}
            assert someremote["type"] == "file"


def test_repos_find_with_invalid_repo():
    with tempfile.TemporaryDirectory() as tmpdir:
        shell(
            f"""
            cd {tmpdir}
            mkdir repo1
            (
                cd ./repo1
                git init
                echo test > test
                git add test
                git commit -m "commit1"
                git remote add origin https://example.com/repo2.git
                git remote add someremote ssh://example.com/repo2.git
            )
            mkdir repo2
            (
                cd ./repo2
                git init
                git co -b main
                echo test > test
                git add test
                git commit -m "commit1"
                git remote add origin https://example.com/repo2.git
            )
            mkdir broken_repo
            (
                cd broken_repo
                echo "broken" > .git
            )
        """
        )

        cmd = grm(["repos", "find", tmpdir])
        assert cmd.returncode == 0
        assert "broken" in cmd.stderr

        output = toml.loads(cmd.stdout)

        assert isinstance(output, dict)
        assert set(output.keys()) == {"trees"}
        assert isinstance(output["trees"], list)
        assert len(output["trees"]) == 1
        for tree in output["trees"]:
            assert set(tree.keys()) == {"root", "repos"}
            assert tree["root"] == tmpdir
            assert isinstance(tree["repos"], list)
            assert len(tree["repos"]) == 2

            repo1 = [r for r in tree["repos"] if r["name"] == "repo1"][0]
            assert repo1["worktree_setup"] is False
            assert isinstance(repo1["remotes"], list)
            assert len(repo1["remotes"]) == 2

            origin = [r for r in repo1["remotes"] if r["name"] == "origin"][0]
            assert set(origin.keys()) == {"name", "type", "url"}
            assert origin["type"] == "https"
            assert origin["url"] == "https://example.com/repo2.git"

            someremote = [r for r in repo1["remotes"] if r["name"] == "someremote"][0]
            assert set(origin.keys()) == {"name", "type", "url"}
            assert someremote["type"] == "ssh"
            assert someremote["url"] == "ssh://example.com/repo2.git"

            repo2 = [r for r in tree["repos"] if r["name"] == "repo2"][0]
            assert repo2["worktree_setup"] is False
            assert isinstance(repo1["remotes"], list)
            assert len(repo2["remotes"]) == 1

            origin = [r for r in repo2["remotes"] if r["name"] == "origin"][0]
            assert set(origin.keys()) == {"name", "type", "url"}
            assert origin["type"] == "https"
            assert origin["url"] == "https://example.com/repo2.git"
