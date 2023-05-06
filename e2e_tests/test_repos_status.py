#!/usr/bin/env python3

from helpers import RepoTree, grm


def test_repos_sync_worktree_clone():
    with RepoTree() as (root, config, repos):
        cmd = grm(["repos", "status", "--config", config])
        assert cmd.returncode == 0
        for repo in repos:
            assert repo in cmd.stdout
