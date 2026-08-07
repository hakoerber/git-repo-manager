#!/usr/bin/env python3

import git
from helpers import EmptyDir, grm, shell


def make_repo(root):
    """
    Create a repository in {root}/repo with two branches:

    * "master", which tracks origin/master
    * "feature", which was pushed without --set-upstream, so origin/feature
      exists, but the branch does not track anything
    """
    shell(f"""
        cd {root}
        git -c init.defaultBranch=master init --bare origin.git
        git -c init.defaultBranch=master clone ./origin.git repo
        cd repo
        echo test > root-commit
        git add root-commit
        git commit -m "root-commit"
        git push --set-upstream origin master
        git switch --create feature
        echo test > feature-commit
        git add feature-commit
        git commit -m "feature-commit"
        git push origin feature
    """)
    return f"{root}/repo"


def test_repos_set_upstream():
    with EmptyDir() as root:
        repo_dir = make_repo(root)
        assert git.Repo(repo_dir).heads["feature"].tracking_branch() is None

        cmd = grm(["repos", "set-upstream"], cwd=repo_dir)
        assert cmd.returncode == 0
        assert "feature" in cmd.stdout

        repo = git.Repo(repo_dir)
        assert repo.heads["feature"].tracking_branch().name == "origin/feature"
        assert repo.heads["master"].tracking_branch().name == "origin/master"


def test_repos_set_upstream_without_candidate():
    with EmptyDir() as root:
        repo_dir = make_repo(root)
        shell(f"cd {repo_dir} && git switch --create local-only")

        cmd = grm(["repos", "set-upstream"], cwd=repo_dir)
        assert cmd.returncode == 0

        repo = git.Repo(repo_dir)
        assert repo.heads["local-only"].tracking_branch() is None
        assert repo.heads["feature"].tracking_branch().name == "origin/feature"


def test_repos_set_upstream_ambiguous():
    with EmptyDir() as root:
        repo_dir = make_repo(root)
        shell(f"""
            cd {root}
            git -c init.defaultBranch=master init --bare other.git
            cd repo
            git remote add other file://{root}/other.git
            git push other feature
        """)

        cmd = grm(["repos", "set-upstream"], cwd=repo_dir)
        assert cmd.returncode == 2
        assert "multiple remotes" in cmd.stderr

        assert git.Repo(repo_dir).heads["feature"].tracking_branch() is None


def test_repos_set_upstream_with_config():
    with EmptyDir() as root:
        repo_dir = make_repo(root)

        config = f"{root}/config.toml"
        with open(config, "w") as f:
            f.write(f"""
                [[trees]]
                root = "{root}"

                [[trees.repos]]
                name = "repo"
            """)

        cmd = grm(["repos", "set-upstream", "--config", config])
        assert cmd.returncode == 0

        assert (
            git.Repo(repo_dir).heads["feature"].tracking_branch().name
            == "origin/feature"
        )
