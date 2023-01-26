#!/usr/bin/env python3

import hashlib
import inspect
import os
import os.path
import shutil
import subprocess
import tempfile

import git

binary = os.environ["GRM_BINARY"]


def funcname():
    return inspect.stack()[1][3]


def copytree(src, dest):
    shutil.copytree(src, dest, dirs_exist_ok=True)


def get_temporary_directory(dir=None):
    return tempfile.TemporaryDirectory(dir=dir)


def grm(args, cwd=None, is_invalid=False):
    cmd = subprocess.run([binary] + args, cwd=cwd, capture_output=True, text=True)
    if not is_invalid:
        assert "usage" not in cmd.stderr.lower()
    print(f"grmcmd: {args}")
    print(f"stdout:\n{cmd.stdout}")
    print(f"stderr:\n{cmd.stderr}")
    assert "secret-token:" not in cmd.stdout
    assert "secret-token:" not in cmd.stderr
    assert "panicked" not in cmd.stderr
    return cmd


def shell(script):
    script = "set -o errexit\nset -o nounset\nset -o pipefail\n" + script
    cmd = subprocess.run(["bash"], input=script, text=True, capture_output=True)
    if cmd.returncode != 0:
        print(cmd.stdout)
        print(cmd.stderr)
    cmd.check_returncode()


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
        checksum = hashlib.md5()

        # A note about bytes(). You may think that it converts something to
        # bytes (akin to str()). But it actually creates a list of zero bytes
        # with the length specified by the parameter.
        #
        # This is kinda couterintuitive to me:
        #
        # str(5)   => '5'
        # bytes(5) => b'\x00\x00\x00\x00\x00'
        def int_to_bytes(i):
            return i.to_bytes((i.bit_length() + 7) // 8, byteorder="big")

        # lstat() instead of stat() so symlinks are not followed. So symlinks
        # are treated as-is and will also be checked for changes.
        stat = os.lstat(path)

        # Note that the list of attributes does not include any timings except
        # mtime.
        for s in [
            stat.st_mode,  # type & permission bits
            stat.st_ino,  # inode
            stat.st_uid,
            stat.st_gid,
            # it's a float in seconds, so this gives us ~1us precision
            int(stat.st_mtime * 1e6),
        ]:
            checksum.update(int_to_bytes(s))
        return checksum.digest()

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

    def __enter__(self):
        self.tmpdir = get_temporary_directory(self.dir)
        self.remote_1 = get_temporary_directory()
        self.remote_2 = get_temporary_directory()
        cmd = f"""
            cd {self.tmpdir.name}
            git -c init.defaultBranch=master init
            echo test > root-commit
            git add root-commit
            git commit -m "root-commit"
            git remote add origin file://{self.remote_1.name}
            git remote add otherremote file://{self.remote_2.name}
        """

        shell(cmd)
        return self.tmpdir.name

    def __exit__(self, exc_type, exc_val, exc_tb):
        pass


class TempGitRemote:
    obj = {}

    def __init__(self, tmpdir, remoteid=None):
        self.tmpdir = tmpdir
        self.remoteid = remoteid

    @classmethod
    def get(cls, cachekey=None, initfunc=None):
        if cachekey is None:
            tmpdir = get_temporary_directory()
            shell(
                f"""
                cd {tmpdir.name}
                git -c init.defaultBranch=master init --bare
            """
            )
            newobj = cls(tmpdir)
            remoteid = None
            if initfunc is not None:
                remoteid = newobj.init(initfunc)
            newobj.remoteid = remoteid
            return newobj, remoteid
        else:
            if cachekey not in cls.obj:
                tmpdir = get_temporary_directory()
                shell(
                    f"""
                    cd {tmpdir.name}
                    git -c init.defaultBranch=master init --bare
                """
                )
                newobj = cls(tmpdir)
                remoteid = newobj.init(initfunc)
                newobj.remoteid = remoteid
                cls.obj[cachekey] = newobj
            return cls.clone(cls.obj[cachekey])

    @classmethod
    def clone(cls, source):
        new_remote = get_temporary_directory()
        copytree(source.tmpdir.name, new_remote.name)
        return cls(new_remote, source.remoteid), source.remoteid

    def init(self, func):
        return func(self.tmpdir.name)

    def __enter__(self):
        return self.tmpdir

    def __exit__(self, exc_type, exc_val, exc_tb):
        pass


class TempGitRepositoryWorktree:
    obj = {}

    def __init__(self, remotes, tmpdir, commit, remote1, remote2, remote1id, remote2id):
        self.remotes = remotes
        self.tmpdir = tmpdir
        self.commit = commit
        self.remote1 = remote1
        self.remote2 = remote2
        self.remote1id = remote1id
        self.remote2id = remote2id

    @classmethod
    def get(cls, cachekey, branch=None, remotes=2, basedir=None, remote_setup=None):
        if cachekey not in cls.obj:
            tmpdir = get_temporary_directory()
            shell(
                f"""
                cd {tmpdir.name}
                git -c init.defaultBranch=master init
                echo test > root-commit-in-worktree-1
                git add root-commit-in-worktree-1
                git commit -m "root-commit-in-worktree-1"
                echo test > root-commit-in-worktree-2
                git add root-commit-in-worktree-2
                git commit -m "root-commit-in-worktree-2"

                git ls-files | xargs rm -rf
                mv .git .git-main-working-tree
                git --git-dir .git-main-working-tree config core.bare true
            """
            )

            repo = git.Repo(f"{tmpdir.name}/.git-main-working-tree")

            commit = repo.head.commit.hexsha
            if branch is not None:
                repo.create_head(branch)

            remote1 = None
            remote2 = None
            remote1id = None
            remote2id = None

            if remotes >= 1:
                cachekeyremote, initfunc = (remote_setup or ((None, None),))[0]
                remote1, remote1id = TempGitRemote.get(
                    cachekey=cachekeyremote, initfunc=initfunc
                )
                remote1 = remote1
                remote1id = remote1id
                shell(
                    f"""
                    cd {tmpdir.name}
                    git --git-dir .git-main-working-tree remote add origin file://{remote1.tmpdir.name}
                """
                )
                repo.remotes.origin.fetch()
                repo.remotes.origin.push("master")

            if remotes >= 2:
                cachekeyremote, initfunc = (remote_setup or (None, (None, None)))[1]
                remote2, remote2id = TempGitRemote.get(
                    cachekey=cachekeyremote, initfunc=initfunc
                )
                remote2 = remote2
                remote2id = remote2id
                shell(
                    f"""
                    cd {tmpdir.name}
                    git --git-dir .git-main-working-tree remote add otherremote file://{remote2.tmpdir.name}
                """
                )
                repo.remotes.otherremote.fetch()
                repo.remotes.otherremote.push("master")

            cls.obj[cachekey] = cls(
                remotes, tmpdir, commit, remote1, remote2, remote1id, remote2id
            )

        return cls.clone(cls.obj[cachekey], remote_setup=remote_setup)

    @classmethod
    def clone(cls, source, remote_setup):
        newdir = get_temporary_directory()

        copytree(source.tmpdir.name, newdir.name)

        remote1 = None
        remote2 = None
        remote1id = None
        remote2id = None
        repo = git.Repo(os.path.join(newdir.name, ".git-main-working-tree"))
        if source.remotes >= 1:
            cachekey, initfunc = (remote_setup or ((None, None),))[0]
            remote1, remote1id = TempGitRemote.get(cachekey=cachekey, initfunc=initfunc)
            if remote1id != source.remote1id:
                repo.remotes.origin.fetch()
                repo.remotes.origin.push("master")
        if source.remotes >= 2:
            cachekey, initfunc = (remote_setup or (None, (None, None)))[1]
            remote2, remote2id = TempGitRemote.get(cachekey=cachekey, initfunc=initfunc)
            if remote2id != source.remote2id:
                repo.remotes.otherremote.fetch()
                repo.remotes.otherremote.push("master")

        return cls(
            source.remotes,
            newdir,
            source.commit,
            remote1,
            remote2,
            remote1id,
            remote2id,
        )

    def __enter__(self):
        return (self.tmpdir.name, self.commit)

    def __exit__(self, exc_type, exc_val, exc_tb):
        pass


class RepoTree:
    def __init__(self):
        pass

    def __enter__(self):
        self.root = get_temporary_directory()
        self.config = tempfile.NamedTemporaryFile()
        with open(self.config.name, "w") as f:
            f.write(
                f"""
                [[trees]]
                root = "{self.root.name}"

                [[trees.repos]]
                name = "test"

                [[trees.repos]]
                name = "test_worktree"
                worktree_setup = true
            """
            )

        cmd = grm(["repos", "sync", "config", "--config", self.config.name])
        assert cmd.returncode == 0
        return (self.root.name, self.config.name, ["test", "test_worktree"])

    def __exit__(self, exc_type, exc_val, exc_tb):
        del self.root
        del self.config


class EmptyDir:
    def __init__(self):
        pass

    def __enter__(self):
        self.tmpdir = get_temporary_directory()
        return self.tmpdir.name

    def __exit__(self, exc_type, exc_val, exc_tb):
        del self.tmpdir


class NonGitDir:
    def __init__(self):
        pass

    def __enter__(self):
        self.tmpdir = get_temporary_directory()
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
        self.tmpdir = get_temporary_directory()
        shell(
            f"""
            cd {self.tmpdir.name}
            git -c init.defaultBranch=master init
            echo test > root-commit-in-remote-1
            git add root-commit-in-remote-1
            git commit -m "root-commit-in-remote-1"
            echo test > root-commit-in-remote-2
            git add root-commit-in-remote-2
            git commit -m "root-commit-in-remote-2"
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
