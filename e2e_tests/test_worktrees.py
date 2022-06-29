#!/usr/bin/env python3

from helpers import *

import git
import pytest
import datetime

import os.path


@pytest.mark.parametrize(
    "config_setup",
    (
        (False, False, False),
        (True, False, False),
        (True, False, True),
        (True, True, False),
        (True, True, True),
    ),
)
@pytest.mark.parametrize("explicit_notrack", [True, False])
@pytest.mark.parametrize("explicit_track", [True, False])
@pytest.mark.parametrize(
    "local_branch_setup", ((False, False), (True, False), (True, True))
)
@pytest.mark.parametrize("remote_branch_already_exists", [True, False])
@pytest.mark.parametrize("remote_branch_with_prefix_already_exists", [True, False])
@pytest.mark.parametrize(
    "remote_setup",
    (
        (0, "origin", False),
        (1, "origin", False),
        (2, "origin", False),
        (2, "otherremote", False),
        (2, "origin", True),
        (2, "otherremote", True),
    ),
)
@pytest.mark.parametrize("track_differs_from_existing_branch_upstream", [True, False])
@pytest.mark.parametrize("worktree_with_slash", [True, False])
def test_worktree_add(
    config_setup,
    explicit_notrack,
    explicit_track,
    local_branch_setup,
    remote_branch_already_exists,
    remote_branch_with_prefix_already_exists,
    remote_setup,
    track_differs_from_existing_branch_upstream,
    worktree_with_slash,
):
    (remote_count, default_remote, remotes_differ) = remote_setup
    (
        config_enabled,
        config_has_default_remote_prefix,
        config_has_default_track_enabled,
    ) = config_setup
    (local_branch_exists, local_branch_has_tracking_branch) = local_branch_setup
    has_remotes = True if remote_count > 0 else False

    if worktree_with_slash:
        worktree_name = "dir/nested/test"
    else:
        worktree_name = "test"

    if track_differs_from_existing_branch_upstream:
        explicit_track_branch_name = f"{default_remote}/somethingelse"
    else:
        explicit_track_branch_name = f"{default_remote}/{worktree_name}"

    timestamp = datetime.datetime.now().replace(microsecond=0).isoformat()
    # GitPython has some weird behaviour here. It is not possible to use kwargs
    # to set the commit and author date.
    #
    # `committer_date=x` (which is documented) does not work, as `git commit`
    # does not accept --committer-date
    #
    # `author_date=x` does not work, as it's now called --date in `git commit`
    #
    # `date=x` should work, but is refused by GitPython, as it does not know
    # about the new behaviour in `git commit`
    #
    # Fortunately, there are env variables that control those timestamps.
    os.environ["GIT_COMMITTER_DATE"] = str(timestamp)
    os.environ["GIT_AUTHOR_DATE"] = str(timestamp)

    def setup_remote1(directory):
        if remote_branch_already_exists:
            with tempfile.TemporaryDirectory() as cloned:
                repo = git.Repo.clone_from(directory, cloned)
                newfile = os.path.join(cloned, "change")
                open(newfile, "w").close()
                repo.index.add([newfile])
                repo.index.commit("commit")
                repo.remotes.origin.push(f"HEAD:{worktree_name}", force=True)

        if remote_branch_with_prefix_already_exists:
            with tempfile.TemporaryDirectory() as cloned:
                repo = git.Repo.clone_from(directory, cloned)
                newfile = os.path.join(cloned, "change2")
                open(newfile, "w").close()
                repo.index.add([newfile])
                repo.index.commit("commit")
                repo.remotes.origin.push(f"HEAD:myprefix/{worktree_name}", force=True)

        return "_".join(
            [
                str(worktree_with_slash),
                str(remote_branch_already_exists),
                str(remote_branch_with_prefix_already_exists),
                str(remotes_differ),
            ]
        )

    def setup_remote2(directory):
        if remote_branch_already_exists:
            with tempfile.TemporaryDirectory() as cloned:
                repo = git.Repo.clone_from(directory, cloned)
                newfile = os.path.join(cloned, "change")
                open(newfile, "w").close()
                repo.index.add([newfile])
                repo.index.commit("commit")
                if remotes_differ:
                    newfile = os.path.join(cloned, "change_on_second_remote")
                    open(newfile, "w").close()
                    repo.index.add([newfile])
                    repo.index.commit("commit_on_second_remote")
                repo.remotes.origin.push(f"HEAD:{worktree_name}", force=True)

        if remote_branch_with_prefix_already_exists:
            with tempfile.TemporaryDirectory() as cloned:
                repo = git.Repo.clone_from(directory, cloned)
                newfile = os.path.join(cloned, "change2")
                open(newfile, "w").close()
                repo.index.add([newfile])
                repo.index.commit("commit")
                if remotes_differ:
                    newfile = os.path.join(cloned, "change_on_second_remote2")
                    open(newfile, "w").close()
                    repo.index.add([newfile])
                    repo.index.commit("commit_on_second_remote2")
                repo.remotes.origin.push(f"HEAD:myprefix/{worktree_name}", force=True)

        return "_".join(
            [
                str(worktree_with_slash),
                str(remote_branch_already_exists),
                str(remote_branch_with_prefix_already_exists),
                str(remotes_differ),
            ]
        )

    cachefn = lambda nr: "_".join(
        [
            str(nr),
            str(default_remote),
            str(local_branch_exists),
            str(remote_branch_already_exists),
            str(remote_branch_with_prefix_already_exists),
            str(remote_count),
            str(remotes_differ),
            str(worktree_name),
        ]
    )
    remote1_cache_key = cachefn(1)
    remote2_cache_key = cachefn(2)

    cachekey = "_".join(
        [
            str(local_branch_exists),
            str(local_branch_has_tracking_branch),
            str(remote_branch_already_exists),
            str(remote_branch_with_prefix_already_exists),
            str(remote_count),
            str(remotes_differ),
            str(worktree_name),
        ]
    )

    with TempGitRepositoryWorktree.get(
        cachekey=cachekey,
        branch=worktree_name if local_branch_exists else None,
        remotes=remote_count,
        remote_setup=[
            [remote1_cache_key, setup_remote1],
            [remote2_cache_key, setup_remote2],
        ],
    ) as (base_dir, initial_commit):
        repo = git.Repo(os.path.join(base_dir, ".git-main-working-tree"))

        if config_enabled:
            with open(os.path.join(base_dir, "grm.toml"), "w") as f:
                f.write(
                    f"""
                        [track]
                        default = {str(config_has_default_track_enabled).lower()}
                        default_remote = "{default_remote}"
                        """
                )

                if config_has_default_remote_prefix:
                    f.write(
                        """
                    default_remote_prefix = "myprefix"
                    """
                    )

        if local_branch_exists:
            if has_remotes and local_branch_has_tracking_branch:
                origin = repo.remote(default_remote)
                if remote_count >= 2:
                    otherremote = repo.remote("otherremote")
                br = list(filter(lambda x: x.name == worktree_name, repo.branches))[0]
                assert os.path.exists(base_dir)
                if track_differs_from_existing_branch_upstream:
                    origin.push(
                        f"{worktree_name}:someothername", force=True, set_upstream=True
                    )
                    if remote_count >= 2:
                        otherremote.push(
                            f"{worktree_name}:someothername",
                            force=True,
                            set_upstream=True,
                        )
                    br.set_tracking_branch(
                        list(
                            filter(
                                lambda x: x.remote_head == "someothername", origin.refs
                            )
                        )[0]
                    )
                else:
                    origin.push(
                        f"{worktree_name}:{worktree_name}",
                        force=True,
                        set_upstream=True,
                    )
                    if remote_count >= 2:
                        otherremote.push(
                            f"{worktree_name}:{worktree_name}",
                            force=True,
                            set_upstream=True,
                        )
                    br.set_tracking_branch(
                        list(
                            filter(
                                lambda x: x.remote_head == worktree_name, origin.refs
                            )
                        )[0]
                    )

        args = ["wt", "add", worktree_name]
        if explicit_track:
            args.extend(["--track", explicit_track_branch_name])
        if explicit_notrack:
            args.extend(["--no-track"])
        cmd = grm(args, cwd=base_dir)
        if explicit_track and not explicit_notrack and not has_remotes:
            assert cmd.returncode != 0
            assert f'remote "{default_remote}" not found' in cmd.stderr.lower()
            return
        assert cmd.returncode == 0

        assert len(cmd.stdout.strip().split("\n")) == 1
        assert f"worktree {worktree_name} created" in cmd.stdout.lower()

        def check_deviation_error(base):
            if (
                not local_branch_exists
                and (explicit_notrack or (not explicit_notrack and not explicit_track))
                and (
                    remote_branch_already_exists
                    or (
                        config_enabled
                        and config_has_default_remote_prefix
                        and remote_branch_with_prefix_already_exists
                    )
                )
                and remote_count >= 2
                and remotes_differ
            ):
                assert (
                    f"branch exists on multiple remotes, but they deviate"
                    in cmd.stderr.lower()
                )
                assert len(cmd.stderr.strip().split("\n")) == base + 1
            else:
                if base == 0:
                    assert len(cmd.stderr) == base
                else:
                    assert len(cmd.stderr.strip().split("\n")) == base

        if explicit_track and explicit_notrack:
            assert "--track will be ignored" in cmd.stderr.lower()
            check_deviation_error(1)
        else:
            check_deviation_error(0)

        files = os.listdir(base_dir)
        if config_enabled is True:
            if worktree_with_slash:
                assert set(files) == {".git-main-working-tree", "grm.toml", "dir"}
            else:
                assert set(files) == {".git-main-working-tree", "grm.toml", "test"}
            assert len(files) == 3
            if worktree_with_slash:
                assert set(files) == {".git-main-working-tree", "grm.toml", "dir"}
                assert set(os.listdir(os.path.join(base_dir, "dir"))) == {"nested"}
                assert set(os.listdir(os.path.join(base_dir, "dir/nested"))) == {"test"}
            else:
                assert set(files) == {".git-main-working-tree", "grm.toml", "test"}
        else:
            assert len(files) == 2
            if worktree_with_slash:
                assert set(files) == {".git-main-working-tree", "dir"}
                assert set(os.listdir(os.path.join(base_dir, "dir"))) == {"nested"}
                assert set(os.listdir(os.path.join(base_dir, "dir/nested"))) == {"test"}
            else:
                assert set(files) == {".git-main-working-tree", "test"}

        repo = git.Repo(os.path.join(base_dir, worktree_name))
        assert not repo.bare
        # assert not repo.is_dirty()
        assert str(repo.head.ref) == worktree_name

        local_commit = repo.head.commit.hexsha

        if not has_remotes:
            assert local_commit == initial_commit
        elif local_branch_exists:
            assert local_commit == initial_commit
        elif explicit_track and not explicit_notrack:
            assert local_commit == repo.commit(explicit_track_branch_name).hexsha
        elif explicit_notrack:
            if config_enabled and config_has_default_remote_prefix:
                if remote_branch_with_prefix_already_exists:
                    assert (
                        local_commit
                        == repo.commit(
                            f"{default_remote}/myprefix/{worktree_name}"
                        ).hexsha
                    )
                elif remote_branch_already_exists:
                    assert (
                        local_commit
                        == repo.commit(f"{default_remote}/{worktree_name}").hexsha
                    )
                else:
                    assert local_commit == initial_commit
            elif remote_count == 1:
                if config_enabled and config_has_default_remote_prefix:
                    if remote_branch_with_prefix_already_exists:
                        assert (
                            local_commit
                            == repo.commit(
                                f"{default_remote}/myprefix/{worktree_name}"
                            ).hexsha
                        )
                    elif remote_branch_already_exists:
                        assert (
                            local_commit
                            == repo.commit(f"{default_remote}/{worktree_name}").hexsha
                        )
                    else:
                        assert local_commit == initial_commit
                elif remote_branch_already_exists:
                    assert (
                        local_commit
                        == repo.commit(f"{default_remote}/{worktree_name}").hexsha
                    )
                else:
                    assert local_commit == initial_commit
            elif remotes_differ:
                if config_enabled:  # we have a default remote
                    if (
                        config_has_default_remote_prefix
                        and remote_branch_with_prefix_already_exists
                    ):
                        assert (
                            local_commit
                            == repo.commit(
                                f"{default_remote}/myprefix/{worktree_name}"
                            ).hexsha
                        )
                    elif remote_branch_already_exists:
                        assert (
                            local_commit
                            == repo.commit(f"{default_remote}/{worktree_name}").hexsha
                        )
                    else:
                        assert local_commit == initial_commit
                else:
                    assert local_commit == initial_commit

            else:
                if config_enabled and config_has_default_remote_prefix:
                    if remote_branch_with_prefix_already_exists:
                        assert (
                            local_commit
                            == repo.commit(
                                f"{default_remote}/myprefix/{worktree_name}"
                            ).hexsha
                        )
                    elif remote_branch_already_exists:
                        assert (
                            local_commit
                            == repo.commit(f"{default_remote}/{worktree_name}").hexsha
                        )
                    else:
                        assert local_commit == initial_commit

        elif config_enabled:
            if not config_has_default_remote_prefix:
                if config_has_default_track_enabled:
                    assert (
                        local_commit
                        == repo.commit(f"{default_remote}/{worktree_name}").hexsha
                    )
                else:
                    if remote_branch_already_exists:
                        assert (
                            local_commit
                            == repo.commit(f"{default_remote}/{worktree_name}").hexsha
                        )
                    else:
                        assert local_commit == initial_commit
            else:
                if remote_branch_with_prefix_already_exists:
                    assert (
                        local_commit
                        == repo.commit(
                            f"{default_remote}/myprefix/{worktree_name}"
                        ).hexsha
                    )
                elif remote_branch_already_exists:
                    assert (
                        local_commit
                        == repo.commit(f"{default_remote}/{worktree_name}").hexsha
                    )
                elif config_has_default_track_enabled:
                    assert (
                        local_commit
                        == repo.commit(
                            f"{default_remote}/myprefix/{worktree_name}"
                        ).hexsha
                    )
                else:
                    assert local_commit == initial_commit
        elif remote_branch_already_exists and not remotes_differ:
            assert (
                local_commit == repo.commit(f"{default_remote}/{worktree_name}").hexsha
            )
        else:
            assert local_commit == initial_commit

        # Check whether tracking is ok
        if not has_remotes:
            assert repo.active_branch.tracking_branch() is None
        elif explicit_notrack:
            if local_branch_exists and local_branch_has_tracking_branch:
                if track_differs_from_existing_branch_upstream:
                    assert (
                        str(repo.active_branch.tracking_branch())
                        == f"{default_remote}/someothername"
                    )
                else:
                    assert (
                        str(repo.active_branch.tracking_branch())
                        == f"{default_remote}/{worktree_name}"
                    )
            else:
                assert repo.active_branch.tracking_branch() is None
        elif explicit_track:
            assert (
                str(repo.active_branch.tracking_branch()) == explicit_track_branch_name
            )
        elif config_enabled and config_has_default_track_enabled:
            if config_has_default_remote_prefix:
                assert (
                    str(repo.active_branch.tracking_branch())
                    == f"{default_remote}/myprefix/{worktree_name}"
                )
            else:
                assert (
                    str(repo.active_branch.tracking_branch())
                    == f"{default_remote}/{worktree_name}"
                )
        elif local_branch_exists and local_branch_has_tracking_branch:
            if track_differs_from_existing_branch_upstream:
                assert (
                    str(repo.active_branch.tracking_branch())
                    == f"{default_remote}/someothername"
                )
            else:
                assert (
                    str(repo.active_branch.tracking_branch())
                    == f"{default_remote}/{worktree_name}"
                )
        else:
            assert repo.active_branch.tracking_branch() is None


def test_worktree_add_invalid_name():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        for worktree_name in [
            "/absolute/path",
            "trailingslash/",
            "with spaces",
            "with\t tabs",
            "with\nnewline",
        ]:
            args = ["wt", "add", worktree_name]
            cmd = grm(args, cwd=base_dir)
            assert cmd.returncode != 0
            assert not os.path.exists(worktree_name)
            assert not os.path.exists(os.path.join(base_dir, worktree_name))
            assert "invalid worktree name" in str(cmd.stderr.lower())


def test_worktree_add_invalid_track():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        for track in ["/absolute/path", "trailingslash/", "/"]:
            args = ["wt", "add", "foo", "--track", track]
            cmd = grm(args, cwd=base_dir)
            assert cmd.returncode != 0
            assert len(cmd.stderr.strip().split("\n")) == 1
            assert not os.path.exists("foo")
            assert not os.path.exists(os.path.join(base_dir, "foo"))
            assert "tracking branch" in str(cmd.stderr.lower())


@pytest.mark.parametrize("use_track", [True, False])
@pytest.mark.parametrize("use_configuration", [True, False])
@pytest.mark.parametrize("use_configuration_default", [True, False])
def test_worktree_add_invalid_remote_name(
    use_track, use_configuration, use_configuration_default
):
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        if use_configuration:
            with open(os.path.join(base_dir, "grm.toml"), "w") as f:
                f.write(
                    f"""
                [track]
                default = {str(use_configuration_default).lower()}
                default_remote = "thisremotedoesnotexist"
                """
                )

        args = ["wt", "add", "foo"]
        if use_track:
            args.extend(["--track", "thisremotedoesnotexist/master"])

        cmd = grm(args, cwd=base_dir)

        if use_track or (use_configuration and use_configuration_default):
            assert cmd.returncode != 0
            assert "thisremotedoesnotexist" in cmd.stderr
        else:
            assert cmd.returncode == 0
            assert len(cmd.stderr) == 0


def test_worktree_add_into_invalid_subdirectory():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "/dir/test"], cwd=base_dir)
        assert cmd.returncode == 1
        assert "dir" not in os.listdir(base_dir)
        assert "dir" not in os.listdir("/")

        cmd = grm(["wt", "add", "dir/"], cwd=base_dir)
        assert cmd.returncode == 1
        assert "dir" not in os.listdir(base_dir)


def test_worktree_delete():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" not in os.listdir(base_dir)

        cmd = grm(["wt", "add", "check"], cwd=base_dir)
        assert cmd.returncode == 0
        repo = git.Repo(os.path.join(base_dir, ".git-main-working-tree"))
        assert "test" not in [str(b) for b in repo.branches]


@pytest.mark.parametrize("has_other_worktree", [True, False])
def test_worktree_delete_in_subfolder(has_other_worktree):
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "dir/test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "dir" in os.listdir(base_dir)

        if has_other_worktree is True:
            cmd = grm(
                ["wt", "add", "dir/test2", "--track", "origin/test"], cwd=base_dir
            )
            assert cmd.returncode == 0
            assert {"test", "test2"} == set(os.listdir(os.path.join(base_dir, "dir")))
        else:
            assert {"test"} == set(os.listdir(os.path.join(base_dir, "dir")))

        cmd = grm(["wt", "delete", "dir/test"], cwd=base_dir)
        assert cmd.returncode == 0
        if has_other_worktree is True:
            assert {"test2"} == set(os.listdir(os.path.join(base_dir, "dir")))
        else:
            assert "dir" not in os.listdir(base_dir)


def test_worktree_delete_refusal_no_tracking_branch():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_refusal_uncommited_changes_new_file():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(f"cd {base_dir}/test && touch changed_file")

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_refusal_uncommited_changes_changed_file():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(f"cd {base_dir}/test && git ls-files | shuf | head | xargs rm -rf")

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_refusal_uncommited_changes_deleted_file():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"cd {base_dir}/test && git ls-files | shuf | head | while read f ; do echo $RANDOM > $f ; done"
        )

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_refusal_commited_changes():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f'cd {base_dir}/test && touch changed_file && git add changed_file && git commit -m "commitmsg"'
        )

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_refusal_tracking_branch_mismatch():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0

        shell(
            f"cd {base_dir}/test && git push origin test && git reset --hard origin/test^"
        )

        before = checksum_directory(f"{base_dir}/test")
        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode != 0
        stderr = cmd.stderr.lower()
        assert "refuse" in stderr or "refusing" in stderr
        assert "test" in os.listdir(base_dir)

        after = checksum_directory(f"{base_dir}/test")
        assert before == after


def test_worktree_delete_force_refusal():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test"], cwd=base_dir)
        assert cmd.returncode == 0

        cmd = grm(["wt", "delete", "test", "--force"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" not in os.listdir(base_dir)


def test_worktree_add_delete_add():
    with TempGitRepositoryWorktree.get(funcname()) as (base_dir, _commit):
        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)

        cmd = grm(["wt", "delete", "test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" not in os.listdir(base_dir)

        cmd = grm(["wt", "add", "test", "--track", "origin/test"], cwd=base_dir)
        assert cmd.returncode == 0
        assert "test" in os.listdir(base_dir)
