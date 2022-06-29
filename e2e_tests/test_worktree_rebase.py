#!/usr/bin/env python3

from helpers import *

import re

import pytest
import git


@pytest.mark.parametrize("pull", [True, False])
@pytest.mark.parametrize("rebase", [True, False])
@pytest.mark.parametrize("ffable", [True, False])
@pytest.mark.parametrize("has_changes", [True, False])
@pytest.mark.parametrize("stash", [True, False])
def test_worktree_rebase(pull, rebase, ffable, has_changes, stash):
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _root_commit):
        with open(os.path.join(base_dir, "grm.toml"), "w") as f:
            f.write('persistent_branches = ["mybasebranch"]')

        repo = git.Repo(f"{base_dir}/.git-main-working-tree")

        grm(
            ["wt", "add", "mybasebranch", "--track", "origin/mybasebranch"],
            cwd=base_dir,
        )

        shell(
            f"""
            cd {base_dir}/mybasebranch
            echo change > mychange-root
            git add mychange-root
            git commit -m "commit-root"
            echo change > mychange-base-local
            git add mychange-base-local
            git commit -m "commit-in-base-local"
            git push origin mybasebranch
        """
        )

        grm(
            ["wt", "add", "myfeatbranch", "--track", "origin/myfeatbranch"],
            cwd=base_dir,
        )
        shell(
            f"""
            cd {base_dir}/myfeatbranch
            git reset --hard mybasebranch^ # root
            echo change > mychange-feat-local
            git add mychange-feat-local
            git commit -m "commit-in-feat-local"
            git push origin HEAD:myfeatbranch
        """
        )

        grm(["wt", "add", "tmp"], cwd=base_dir)
        shell(
            f"""
            cd {base_dir}/tmp
            git reset --hard mybasebranch
            echo change > mychange-base-remote
            git add mychange-base-remote
            git commit -m "commit-in-base-remote"
            git push origin HEAD:mybasebranch

            git reset --hard myfeatbranch
            echo change > mychange-feat-remote
            git add mychange-feat-remote
            git commit -m "commit-in-feat-remote"
            git push origin HEAD:myfeatbranch
        """
        )

        if not ffable:
            shell(
                f"""
                cd {base_dir}/mybasebranch
                echo change > mychange-base-no-ff
                git add mychange-base-no-ff
                git commit -m "commit-in-base-local-no-ff"

                cd {base_dir}/myfeatbranch
                echo change > mychange-feat-no-ff
                git add mychange-feat-no-ff
                git commit -m "commit-in-feat-local-no-ff"
            """
            )

        if has_changes:
            shell(
                f"""
                cd {base_dir}/myfeatbranch
                echo uncommitedchange > uncommitedchange
            """
            )

        grm(["wt", "delete", "--force", "tmp"], cwd=base_dir)

        repo = git.Repo(f"{base_dir}/.git-main-working-tree")
        if ffable:
            assert repo.commit("mybasebranch~1").message.strip() == "commit-root"
            assert (
                repo.refs.mybasebranch.commit.message.strip() == "commit-in-base-local"
            )
            assert (
                repo.remote("origin").refs.mybasebranch.commit.message.strip()
                == "commit-in-base-remote"
            )
            assert (
                repo.refs.myfeatbranch.commit.message.strip() == "commit-in-feat-local"
            )
            assert (
                repo.remote("origin").refs.myfeatbranch.commit.message.strip()
                == "commit-in-feat-remote"
            )
        else:
            assert (
                repo.commit("mybasebranch").message.strip()
                == "commit-in-base-local-no-ff"
            )
            assert (
                repo.commit("mybasebranch~1").message.strip() == "commit-in-base-local"
            )
            assert repo.commit("mybasebranch~2").message.strip() == "commit-root"
            assert (
                repo.commit("myfeatbranch").message.strip()
                == "commit-in-feat-local-no-ff"
            )
            assert (
                repo.commit("myfeatbranch~1").message.strip() == "commit-in-feat-local"
            )
            assert repo.commit("myfeatbranch~2").message.strip() == "commit-root"
            assert (
                repo.remote("origin").refs.mybasebranch.commit.message.strip()
                == "commit-in-base-remote"
            )
            assert (
                repo.remote("origin").refs.myfeatbranch.commit.message.strip()
                == "commit-in-feat-remote"
            )

        args = ["wt", "rebase"]
        if pull:
            args += ["--pull"]
        if rebase:
            args += ["--rebase"]
        if stash:
            args += ["--stash"]
        cmd = grm(args, cwd=base_dir)

        if rebase and not pull:
            assert cmd.returncode != 0
            assert len(cmd.stderr) != 0
        elif has_changes and not stash:
            assert cmd.returncode != 0
            assert re.match(r".*myfeatbranch.*contains changes.*", cmd.stderr)
        else:
            repo = git.Repo(f"{base_dir}/myfeatbranch")
            if has_changes:
                assert ["uncommitedchange"] == repo.untracked_files
            if pull:
                if rebase:
                    assert cmd.returncode == 0
                    if ffable:
                        assert (
                            repo.commit("HEAD").message.strip()
                            == "commit-in-feat-remote"
                        )
                        assert (
                            repo.commit("HEAD~1").message.strip()
                            == "commit-in-feat-local"
                        )
                        assert (
                            repo.commit("HEAD~2").message.strip()
                            == "commit-in-base-remote"
                        )
                        assert (
                            repo.commit("HEAD~3").message.strip()
                            == "commit-in-base-local"
                        )
                        assert repo.commit("HEAD~4").message.strip() == "commit-root"
                    else:
                        assert (
                            repo.commit("HEAD").message.strip()
                            == "commit-in-feat-local-no-ff"
                        )
                        assert (
                            repo.commit("HEAD~1").message.strip()
                            == "commit-in-feat-remote"
                        )
                        assert (
                            repo.commit("HEAD~2").message.strip()
                            == "commit-in-feat-local"
                        )
                        assert (
                            repo.commit("HEAD~3").message.strip()
                            == "commit-in-base-local-no-ff"
                        )
                        assert (
                            repo.commit("HEAD~4").message.strip()
                            == "commit-in-base-remote"
                        )
                        assert (
                            repo.commit("HEAD~5").message.strip()
                            == "commit-in-base-local"
                        )
                        assert repo.commit("HEAD~6").message.strip() == "commit-root"
                else:
                    if ffable:
                        assert cmd.returncode == 0
                        assert (
                            repo.commit("HEAD").message.strip()
                            == "commit-in-feat-remote"
                        )
                        assert (
                            repo.commit("HEAD~1").message.strip()
                            == "commit-in-feat-local"
                        )
                        assert (
                            repo.commit("HEAD~2").message.strip()
                            == "commit-in-base-remote"
                        )
                        assert (
                            repo.commit("HEAD~3").message.strip()
                            == "commit-in-base-local"
                        )
                        assert repo.commit("HEAD~4").message.strip() == "commit-root"
                    else:
                        assert cmd.returncode != 0
                        assert (
                            repo.commit("HEAD").message.strip()
                            == "commit-in-feat-local-no-ff"
                        )
                        assert (
                            repo.commit("HEAD~1").message.strip()
                            == "commit-in-feat-local"
                        )
                        assert (
                            repo.commit("HEAD~2").message.strip()
                            == "commit-in-base-local-no-ff"
                        )
                        assert (
                            repo.commit("HEAD~3").message.strip()
                            == "commit-in-base-local"
                        )
                        assert repo.commit("HEAD~4").message.strip() == "commit-root"
            else:
                assert cmd.returncode == 0
                if ffable:
                    assert repo.commit("HEAD").message.strip() == "commit-in-feat-local"
                    assert (
                        repo.commit("HEAD~1").message.strip() == "commit-in-base-local"
                    )
                    assert repo.commit("HEAD~2").message.strip() == "commit-root"
                else:
                    assert (
                        repo.commit("HEAD").message.strip()
                        == "commit-in-feat-local-no-ff"
                    )
                    assert (
                        repo.commit("HEAD~1").message.strip() == "commit-in-feat-local"
                    )
                    assert (
                        repo.commit("HEAD~2").message.strip()
                        == "commit-in-base-local-no-ff"
                    )
                    assert (
                        repo.commit("HEAD~3").message.strip() == "commit-in-base-local"
                    )
                    assert repo.commit("HEAD~4").message.strip() == "commit-root"
