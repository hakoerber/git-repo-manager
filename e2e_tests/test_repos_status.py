from helpers import RepoTree, grm, shell


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
