#!/usr/bin/env python3

from helpers import EmptyDir, grm, shell


def test_repos_unmanaged_files():
    with EmptyDir() as root:
        shell(f"""
            cd {root}
            git -c init.defaultBranch=master init repo
            touch stray.txt
            mkdir -p docs/inner
            touch docs/inner/draft.md
            mkdir nested
            git -c init.defaultBranch=master init nested/deep-repo
            touch nested/loose.txt
        """)

        cmd = grm(["repos", "unmanaged-files"], cwd=root)
        assert cmd.returncode == 2
        assert cmd.stdout.splitlines() == [
            f"{root}/docs",
            f"{root}/nested/loose.txt",
            f"{root}/stray.txt",
        ]


def test_repos_unmanaged_files_all_managed():
    with EmptyDir() as root:
        shell(f"""
            cd {root}
            git -c init.defaultBranch=master init repo
            touch repo/tracked-later
        """)

        cmd = grm(["repos", "unmanaged-files"], cwd=root)
        assert cmd.returncode == 0
        assert cmd.stdout.strip() == ""


def test_repos_unmanaged_files_root_is_repo():
    with EmptyDir() as root:
        shell(f"""
            cd {root}
            git -c init.defaultBranch=master init .
            touch stray.txt
        """)

        cmd = grm(["repos", "unmanaged-files"], cwd=root)
        assert cmd.returncode == 0
        assert cmd.stdout.strip() == ""


def test_repos_unmanaged_files_with_config():
    with EmptyDir() as root:
        shell(f"""
            cd {root}
            mkdir tree
            cd tree
            git -c init.defaultBranch=master init repo
            touch stray.txt
        """)

        config = f"{root}/config.toml"
        with open(config, "w") as f:
            f.write(f"""
                [[trees]]
                root = "{root}/tree"

                [[trees.repos]]
                name = "repo"
            """)

        cmd = grm(["repos", "unmanaged-files", "--config", config])
        assert cmd.returncode == 2
        assert cmd.stdout.splitlines() == [f"{root}/tree/stray.txt"]
