from helpers import RepoTree, grm


def test_repos_sync_worktree_clone():
    with RepoTree() as (_root, config, repos):
        cmd = grm(["repos", "status", "--config", config])
        assert cmd.returncode == 0
        assert "does not exist" not in cmd.stderr
        for repo in repos:
            assert repo in cmd.stdout
