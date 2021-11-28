#!/usr/bin/env python3

import os
import os.path
import subprocess
import tempfile
import hashlib

import git

binary = os.path.join(
    os.path.dirname(os.path.dirname(os.path.realpath(__file__))), "target/release/grm"
)


def grm(args, cwd=None, is_invalid=False):
    cmd = subprocess.run([binary] + args, cwd=cwd, capture_output=True, text=True)
    if not is_invalid:
        assert "USAGE" not in cmd.stderr
    print(f"grmcmd: {args}")
    print(f"stdout:\n{cmd.stdout}")
    print(f"stderr:\n{cmd.stderr}")
    assert "panicked" not in cmd.stderr
    return cmd


def shell(script):
    script = "set -o errexit\nset -o nounset\n" + script
    subprocess.run(["bash"], input=script, text=True, check=True)


def checksum_directory(path):
    """
    Gives a "checksum" of a directory that includes all files & directories
    recursively, including owner/group/permissions. Useful to compare that a
    directory did not change after a command was run.

    The following makes it a bit complicated:

    > Whether or not the lists are sorted depends on the file system.

    - https://docs.python.org/3/library/os.html#os.walk

    This means we have to first get a list of all hashes of files and
    directories, then sort the hashes and then create the hash for the whole
    directory.
    """
    path = os.path.realpath(path)

    hashes = []

    if not os.path.exists(path):
        raise f"{path} not found"

    def get_stat_hash(path):
        stat = bytes(str(os.stat(path).__hash__()), "ascii")
        return stat

    for root, dirs, files in os.walk(path):
        for file in files:
            checksum = hashlib.md5()
            filepath = os.path.join(root, file)
            checksum.update(str.encode(filepath))
            checksum.update(get_stat_hash(filepath))
            with open(filepath, "rb") as f:
                while True:
                    data = f.read(8192)
                    if not data:
                        break
                    checksum.update(data)
            hashes.append(checksum.digest())

        for d in dirs:
            checksum = hashlib.md5()
            dirpath = os.path.join(root, d)
            checksum.update(get_stat_hash(dirpath))
            hashes.append(checksum.digest())

    checksum = hashlib.md5()
    for c in sorted(hashes):
        checksum.update(c)
    return checksum.hexdigest()


class TempGitRepository:
    def __init__(self, dir=None):
        self.dir = dir
        pass

    def __enter__(self):
        self.tmpdir = tempfile.TemporaryDirectory(dir=self.dir)
        self.remote_1_dir = tempfile.TemporaryDirectory()
        self.remote_2_dir = tempfile.TemporaryDirectory()
        shell(
            f"""
            cd {self.tmpdir.name}
            git init
            echo test > test
            git add test
            git commit -m "commit1"
            git remote add origin file://{self.remote_1_dir.name}
            git remote add otherremote file://{self.remote_2_dir.name}
        """
        )
        return self.tmpdir.name

    def __exit__(self, exc_type, exc_val, exc_tb):
        del self.tmpdir
        del self.remote_1_dir
        del self.remote_2_dir


class TempGitRepositoryWorktree:
    def __init__(self):
        pass

    def __enter__(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.remote_1_dir = tempfile.TemporaryDirectory()
        self.remote_2_dir = tempfile.TemporaryDirectory()
        shell(
            f"""
            cd {self.remote_1_dir.name}
            git init --bare
        """
        )
        shell(
            f"""
            cd {self.remote_2_dir.name}
            git init --bare
        """
        )
        shell(
            f"""
            cd {self.tmpdir.name}
            git init
            echo test > test
            git add test
            git commit -m "commit1"
            echo test > test2
            git add test2
            git commit -m "commit2"
            git remote add origin file://{self.remote_1_dir.name}
            git remote add otherremote file://{self.remote_2_dir.name}
            git ls-files | xargs rm -rf
            mv .git .git-main-working-tree
            git --git-dir .git-main-working-tree config core.bare true
        """
        )
        return self.tmpdir.name

    def __exit__(self, exc_type, exc_val, exc_tb):
        del self.tmpdir
        del self.remote_1_dir
        del self.remote_2_dir


class EmptyDir:
    def __init__(self):
        pass

    def __enter__(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        return self.tmpdir.name

    def __exit__(self, exc_type, exc_val, exc_tb):
        del self.tmpdir


class NonGitDir:
    def __init__(self):
        pass

    def __enter__(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        shell(
            f"""
            cd {self.tmpdir.name}
            mkdir testdir
            touch testdir/test
            touch test2
        """
        )
        return self.tmpdir.name

    def __exit__(self, exc_type, exc_val, exc_tb):
        del self.tmpdir


class TempGitFileRemote:
    def __init__(self):
        pass

    def __enter__(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        shell(
            f"""
            cd {self.tmpdir.name}
            git init
            echo test > test
            git add test
            git commit -m "commit1"
            echo test > test2
            git add test2
            git commit -m "commit2"
            git ls-files | xargs rm -rf
            mv .git/* .
            git config core.bare true
        """
        )
        head_commit_sha = git.Repo(self.tmpdir.name).head.commit.hexsha
        return (self.tmpdir.name, head_commit_sha)

    def __exit__(self, exc_type, exc_val, exc_tb):
        del self.tmpdir


class NonExistentPath:
    def __init__(self):
        pass

    def __enter__(self):
        self.dir = "/doesnotexist"
        if os.path.exists(self.dir):
            raise f"{self.dir} exists for some reason"
        return self.dir

    def __exit__(self, exc_type, exc_val, exc_tb):
        pass
