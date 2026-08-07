from helpers import EmptyDir, RepoTree, grm, shell


def make_pushed_repo(root):
    """
    Create a repository in {root}/myrepo whose "master" branch tracks
    origin/master and is fully pushed.
    """
    shell(f"""
        cd {root}
        git -c init.defaultBranch=master init --bare origin.git
        git -c init.defaultBranch=master clone ./origin.git myrepo
        cd myrepo
        echo test > root-commit
        git add root-commit
        git commit -m "root-commit"
        git push --set-upstream origin master
    """)
    return f"{root}/myrepo"


def test_repos_sync_worktree_clone():
    with RepoTree() as (_root, config, repos):
        cmd = grm(["repos", "status", "--config", config])
        assert cmd.returncode == 0
        assert "does not exist" not in cmd.stderr
        for repo in repos:
            assert repo in cmd.stdout


def test_repos_status_dirty_hides_clean_repos():
    with RepoTree() as (root, config, repos):
        cmd = grm(["repos", "status", "--config", config, "--dirty"])
        assert cmd.returncode == 0
        assert "does not exist" not in cmd.stderr
        for repo in repos:
            assert repo not in cmd.stdout
        assert cmd.stdout.strip() == ""


def test_repos_status_dirty_shows_repo_with_uncommited_changes():
    with RepoTree() as (root, config, repos):
        shell(f"cd {root}/test && touch changed_file")

        cmd = grm(["repos", "status", "--config", config, "--dirty"])
        assert cmd.returncode == 0
        assert "does not exist" not in cmd.stderr
        assert "test" in cmd.stdout
        assert "New: 1" in cmd.stdout
        assert "test_worktree" not in cmd.stdout
        assert "test_namespace/test_nested" not in cmd.stdout


def test_repos_status_dirty_shows_repo_with_local_only_branch():
    with RepoTree() as (root, config, repos):
        shell(f"""
            cd {root}/test
            echo test > root-commit
            git add root-commit
            git commit -m "root-commit"
        """)

        cmd = grm(["repos", "status", "--config", config, "--dirty"])
        assert cmd.returncode == 0
        assert "does not exist" not in cmd.stderr
        assert "<!local>" in cmd.stdout
        assert "test_worktree" not in cmd.stdout
        assert "test_namespace/test_nested" not in cmd.stdout


def test_repos_status_dirty_ignores_behind_branch():
    with EmptyDir() as root:
        repo_dir = make_pushed_repo(root)
        shell(f"""
            cd {repo_dir}
            echo test > second-commit
            git add second-commit
            git commit -m "second-commit"
            git push origin master
            git reset --hard HEAD~1
        """)

        cmd = grm(["repos", "status"], cwd=repo_dir)
        assert cmd.returncode == 0
        assert "[-1]" in cmd.stdout

        cmd = grm(["repos", "status", "--dirty"], cwd=repo_dir)
        assert cmd.returncode == 0
        assert "myrepo" not in cmd.stdout


def test_repos_status_dirty_shows_ahead_branch():
    with EmptyDir() as root:
        repo_dir = make_pushed_repo(root)
        shell(f"""
            cd {repo_dir}
            echo test > local-commit
            git add local-commit
            git commit -m "local-commit"
        """)

        cmd = grm(["repos", "status", "--dirty"], cwd=repo_dir)
        assert cmd.returncode == 0
        assert "myrepo" in cmd.stdout
        assert "[+1]" in cmd.stdout


def test_repos_status_dirty_single_repo():
    with RepoTree() as (root, config, repos):
        cmd = grm(["repos", "status", "--dirty"], cwd=f"{root}/test")
        assert cmd.returncode == 0
        assert "test" not in cmd.stdout

        shell(f"cd {root}/test && touch changed_file")

        cmd = grm(["repos", "status", "--dirty"], cwd=f"{root}/test")
        assert cmd.returncode == 0
        assert "test" in cmd.stdout
        assert "New: 1" in cmd.stdout
