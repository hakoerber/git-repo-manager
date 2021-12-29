#!/usr/bin/env python3

from helpers import *

import pytest
import git


def test_worktree_fetch():
    with TempGitRepositoryWorktree() as (base_dir, root_commit):
        with TempGitFileRemote() as (remote_path, _remote_sha):
            shell(
                f"""
                cd {base_dir}
                git --git-dir .git-main-working-tree remote add upstream file://{remote_path}
                git --git-dir .git-main-working-tree push --force upstream master:master
            """
            )

            cmd = grm(["wt", "fetch"], cwd=base_dir)
            assert cmd.returncode == 0

            repo = git.Repo(f"{base_dir}/.git-main-working-tree")
            assert repo.commit("master").hexsha == repo.commit("origin/master").hexsha
            assert repo.commit("master").hexsha == repo.commit("upstream/master").hexsha

            with EmptyDir() as tmp:
                shell(
                    f"""
                    cd {tmp}
                    git clone {remote_path} tmp
                    cd tmp
                    echo change > mychange-remote
                    git add mychange-remote
                    git commit -m "change-remote"
                    git push origin HEAD:master
                """
                )
                remote_commit = git.Repo(f"{tmp}/tmp").commit("master").hexsha

            assert repo.commit("master").hexsha == repo.commit("origin/master").hexsha
            assert repo.commit("master").hexsha == repo.commit("upstream/master").hexsha

            cmd = grm(["wt", "fetch"], cwd=base_dir)
            assert cmd.returncode == 0

            assert repo.commit("master").hexsha == repo.commit("origin/master").hexsha
            assert repo.commit("master").hexsha == root_commit
            assert repo.commit("upstream/master").hexsha == remote_commit


@pytest.mark.parametrize("rebase", [True, False])
@pytest.mark.parametrize("ffable", [True, False])
def test_worktree_pull(rebase, ffable):
    with TempGitRepositoryWorktree() as (base_dir, root_commit):
        with TempGitFileRemote() as (remote_path, _remote_sha):
            shell(
                f"""
                cd {base_dir}
                git --git-dir .git-main-working-tree remote add upstream file://{remote_path}
                git --git-dir .git-main-working-tree push --force upstream master:master
            """
            )

            repo = git.Repo(f"{base_dir}/.git-main-working-tree")
            assert repo.commit("origin/master").hexsha == repo.commit("master").hexsha
            assert repo.commit("upstream/master").hexsha == repo.commit("master").hexsha

            with EmptyDir() as tmp:
                shell(
                    f"""
                    cd {tmp}
                    git clone {remote_path} tmp
                    cd tmp
                    git checkout origin/master
                    echo change > mychange-remote
                    git add mychange-remote
                    git commit -m "change-remote"
                    git push origin HEAD:master
                """
                )
                remote_commit = git.Repo(f"{tmp}/tmp").commit("HEAD").hexsha

                grm(["wt", "add", "master", "--track", "upstream/master"], cwd=base_dir)

                repo = git.Repo(f"{base_dir}/master")
                if not ffable:
                    shell(
                        f"""
                        cd {base_dir}/master
                        echo change > mychange
                        git add mychange
                        git commit -m "local-commit-in-master"
                    """
                    )

                args = ["wt", "pull"]
                if rebase:
                    args += ["--rebase"]
                cmd = grm(args, cwd=base_dir)
                assert cmd.returncode == 0

                assert repo.commit("upstream/master").hexsha == remote_commit
                assert repo.commit("origin/master").hexsha == root_commit
                assert (
                    repo.commit("master").hexsha != repo.commit("origin/master").hexsha
                )

                if not rebase:
                    if ffable:
                        assert (
                            repo.commit("master").hexsha
                            != repo.commit("origin/master").hexsha
                        )
                        assert (
                            repo.commit("master").hexsha
                            == repo.commit("upstream/master").hexsha
                        )
                        assert repo.commit("upstream/master").hexsha == remote_commit
                    else:
                        assert "cannot be fast forwarded" in cmd.stderr
                        assert (
                            repo.commit("master").hexsha
                            != repo.commit("origin/master").hexsha
                        )
                        assert repo.commit("master").hexsha != remote_commit
                        assert repo.commit("upstream/master").hexsha == remote_commit
                else:
                    if ffable:
                        assert (
                            repo.commit("master").hexsha
                            != repo.commit("origin/master").hexsha
                        )
                        assert (
                            repo.commit("master").hexsha
                            == repo.commit("upstream/master").hexsha
                        )
                        assert repo.commit("upstream/master").hexsha == remote_commit
                    else:
                        assert (
                            repo.commit("master").message.strip()
                            == "local-commit-in-master"
                        )
                        assert repo.commit("master~1").hexsha == remote_commit
