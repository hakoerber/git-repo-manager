#!/usr/bin/env python3

import os
import re
import tempfile

import pytest
import toml
import yaml
from helpers import grm

ALTERNATE_DOMAIN = os.environ["ALTERNATE_DOMAIN"]
PROVIDERS = ["github", "gitlab"]


@pytest.mark.parametrize("use_config", [True, False])
def test_repos_find_remote_invalid_provider(use_config):
    if use_config:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                f.write(
                    """
                provider = "thisproviderdoesnotexist"
                token_command = "true"
                root = "/"
                """
                )
            args = ["repos", "find", "config", "--config", config.name]
            cmd = grm(args, is_invalid=True)
    else:
        args = [
            "repos",
            "find",
            "remote",
            "--provider",
            "thisproviderdoesnotexist",
            "--token-command",
            "true",
            "--root",
            "/",
        ]
        cmd = grm(args, is_invalid=True)
    assert cmd.returncode != 0
    assert len(cmd.stdout) == 0
    if not use_config:
        assert re.match(".*invalid value 'thisproviderdoesnotexist' for.*provider", cmd.stderr)


@pytest.mark.parametrize("provider", PROVIDERS)
def test_repos_find_remote_invalid_format(provider):
    cmd = grm(
        [
            "repos",
            "find",
            "remote",
            "--provider",
            provider,
            "--format",
            "invalidformat",
            "--token-command",
            "true",
            "--root",
            "/myroot",
        ],
        is_invalid=True,
    )
    assert cmd.returncode != 0
    assert len(cmd.stdout) == 0
    assert "invalid value 'invalidformat'" in cmd.stderr


@pytest.mark.parametrize("provider", PROVIDERS)
def test_repos_find_remote_token_command_failed(provider):
    cmd = grm(
        [
            "repos",
            "find",
            "remote",
            "--provider",
            provider,
            "--format",
            "yaml",
            "--token-command",
            "false",
            "--root",
            "/myroot",
        ],
        is_invalid=True,
    )
    assert cmd.returncode != 0
    assert len(cmd.stdout) == 0
    assert "token command failed" in cmd.stderr.lower()


@pytest.mark.parametrize("provider", PROVIDERS)
@pytest.mark.parametrize("use_config", [True, False])
def test_repos_find_remote_wrong_token(provider, use_config):
    if use_config:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                f.write(
                    f"""
                provider = "{provider}"
                token_command = "echo wrongtoken"
                root = "/myroot"
                [filters]
                access = true
                """
                )
            args = ["repos", "find", "config", "--config", config.name]
            cmd = grm(args, is_invalid=True)
    else:
        args = [
            "repos",
            "find",
            "remote",
            "--provider",
            provider,
            "--token-command",
            "echo wrongtoken",
            "--root",
            "/myroot",
            "--access",
        ]
        cmd = grm(args, is_invalid=True)

    assert cmd.returncode != 0
    assert len(cmd.stdout) == 0
    assert "bad credentials" in cmd.stderr.lower()


@pytest.mark.parametrize("provider", PROVIDERS)
@pytest.mark.parametrize("default", [True, False])
@pytest.mark.parametrize("configtype", ["toml", "yaml"])
@pytest.mark.parametrize("use_config", [True, False])
def test_repos_find_remote_no_filter(provider, configtype, default, use_config):
    if use_config:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                f.write(
                    f"""
                provider = "{provider}"
                token_command = "echo secret-token:myauthtoken"
                root = "/myroot"
                """
                )
            args = ["repos", "find", "config", "--config", config.name]
            if not default:
                args += ["--format", configtype]
            cmd = grm(args)
    else:
        args = [
            "repos",
            "find",
            "remote",
            "--provider",
            provider,
            "--token-command",
            "echo secret-token:myauthtoken",
            "--root",
            "/myroot",
        ]
        if not default:
            args += ["--format", configtype]
        cmd = grm(args)

    assert cmd.returncode == 0
    assert "did not specify any filters" in cmd.stderr.lower()

    if default or configtype == "toml":
        output = toml.loads(cmd.stdout)
    elif configtype == "yaml":
        output = yaml.safe_load(cmd.stdout)
    else:
        raise NotImplementedError()

    assert isinstance(output, dict)
    assert set(output.keys()) == {"trees"}
    assert isinstance(output["trees"], list)
    assert len(output["trees"]) == 0


@pytest.mark.parametrize("provider", PROVIDERS)
@pytest.mark.parametrize("configtype_default", [True, False])
@pytest.mark.parametrize("configtype", ["toml", "yaml"])
@pytest.mark.parametrize("use_config", [True, False])
def test_repos_find_remote_user_empty(
    provider, configtype, configtype_default, use_config
):
    if use_config:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                cfg = f"""
                provider = "{provider}"
                token_command = "echo secret-token:myauthtoken"
                root = "/myroot"

                [filters]
                users = ["someotheruser"]
                """

                f.write(cfg)
            args = ["repos", "find", "config", "--config", config.name]
            if not configtype_default:
                args += ["--format", configtype]
            cmd = grm(args)
    else:
        args = [
            "repos",
            "find",
            "remote",
            "--provider",
            provider,
            "--token-command",
            "echo secret-token:myauthtoken",
            "--root",
            "/myroot",
            "--user",
            "someotheruser",
        ]

        if not configtype_default:
            args += ["--format", configtype]
        cmd = grm(args)
    assert cmd.returncode == 0
    assert len(cmd.stderr) == 0

    if configtype_default or configtype == "toml":
        output = toml.loads(cmd.stdout)
    elif configtype == "yaml":
        output = yaml.safe_load(cmd.stdout)
    else:
        raise NotImplementedError()

    assert isinstance(output, dict)
    assert set(output.keys()) == {"trees"}
    assert isinstance(output["trees"], list)
    assert len(output["trees"]) == 0


@pytest.mark.parametrize("provider", PROVIDERS)
@pytest.mark.parametrize("configtype_default", [True, False])
@pytest.mark.parametrize("configtype", ["toml", "yaml"])
@pytest.mark.parametrize("worktree_default", [True, False])
@pytest.mark.parametrize("worktree", [True, False])
@pytest.mark.parametrize("use_owner", [True, False])
@pytest.mark.parametrize("force_ssh", [True, False])
@pytest.mark.parametrize("use_alternate_endpoint", [True, False])
@pytest.mark.parametrize("use_config", [True, False])
@pytest.mark.parametrize("override_remote_name", [True, False])
def test_repos_find_remote_user(
    provider,
    configtype,
    configtype_default,
    worktree,
    worktree_default,
    use_owner,
    force_ssh,
    use_alternate_endpoint,
    use_config,
    override_remote_name,
):
    if use_config:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                cfg = f"""
                provider = "{provider}"
                token_command = "echo secret-token:myauthtoken"
                root = "/myroot"
                """

                if use_alternate_endpoint:
                    cfg += f'api_url = "http://{ALTERNATE_DOMAIN}:5000/{provider}"\n'
                if not worktree_default:
                    cfg += f"worktree = {str(worktree).lower()}\n"
                if force_ssh:
                    cfg += "force_ssh = true\n"
                if override_remote_name:
                    cfg += 'remote_name = "otherremote"\n'
                if use_owner:
                    cfg += """
                        [filters]
                        owner = true\n
                    """
                else:
                    cfg += """
                        [filters]
                        users = ["myuser1"]\n
                    """

                print(cfg)
                f.write(cfg)

            args = ["repos", "find", "config", "--config", config.name]
            if not configtype_default:
                args += ["--format", configtype]
            cmd = grm(args)
    else:
        args = [
            "repos",
            "find",
            "remote",
            "--provider",
            provider,
            "--token-command",
            "echo secret-token:myauthtoken",
            "--root",
            "/myroot",
        ]
        if use_owner:
            args += ["--owner"]
        else:
            args += ["--user", "myuser1"]
        if force_ssh:
            args += ["--force-ssh"]
        if override_remote_name:
            args += ["--remote-name", "otherremote"]
        if not worktree_default:
            args += ["--worktree", str(worktree).lower()]
        if use_alternate_endpoint:
            args += ["--api-url", f"http://{ALTERNATE_DOMAIN}:5000/{provider}"]

        if not configtype_default:
            args += ["--format", configtype]
        cmd = grm(args)

    if use_alternate_endpoint and provider == "github":
        assert cmd.returncode != 0
        assert "overriding is not supported for github" in cmd.stderr.lower()
        return

    assert cmd.returncode == 0
    assert len(cmd.stderr) == 0

    if configtype_default or configtype == "toml":
        output = toml.loads(cmd.stdout)
    elif configtype == "yaml":
        output = yaml.safe_load(cmd.stdout)
    else:
        raise NotImplementedError()

    assert isinstance(output, dict)
    assert set(output.keys()) == {"trees"}
    assert isinstance(output["trees"], list)
    assert len(output["trees"]) == 1

    assert set(output["trees"][0].keys()) == {"root", "repos"}
    assert isinstance(output["trees"][0]["repos"], list)
    assert len(output["trees"][0]["repos"]) == 5

    for i in range(1, 6):
        repo = [r for r in output["trees"][0]["repos"] if r["name"] == f"myproject{i}"][
            0
        ]
        assert repo["worktree_setup"] is (not worktree_default and worktree)
        assert isinstance(repo["remotes"], list)
        assert len(repo["remotes"]) == 1
        if override_remote_name:
            assert repo["remotes"][0]["name"] == "otherremote"
        else:
            assert repo["remotes"][0]["name"] == "origin"
        if force_ssh or i == 1:
            assert (
                repo["remotes"][0]["url"]
                == f"ssh://git@example.com/myuser1/myproject{i}.git"
            )
            assert repo["remotes"][0]["type"] == "ssh"
        else:
            assert (
                repo["remotes"][0]["url"]
                == f"https://example.com/myuser1/myproject{i}.git"
            )
            assert repo["remotes"][0]["type"] == "https"


@pytest.mark.parametrize("provider", PROVIDERS)
@pytest.mark.parametrize("configtype_default", [False])
@pytest.mark.parametrize("configtype", ["toml", "yaml"])
@pytest.mark.parametrize("use_alternate_endpoint", [True, False])
@pytest.mark.parametrize("use_config", [True, False])
def test_repos_find_remote_group_empty(
    provider, configtype, configtype_default, use_alternate_endpoint, use_config
):
    if use_config:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                cfg = f"""
                provider = "{provider}"
                token_command = "echo secret-token:myauthtoken"
                root = "/myroot"
                """

                if use_alternate_endpoint:
                    cfg += f'api_url = "http://{ALTERNATE_DOMAIN}:5000/{provider}"\n'
                cfg += """
                    [filters]
                    groups = ["someothergroup"]\n
                """

                f.write(cfg)

            args = ["repos", "find", "config", "--config", config.name]
            if not configtype_default:
                args += ["--format", configtype]
            cmd = grm(args)
    else:
        args = [
            "repos",
            "find",
            "remote",
            "--provider",
            provider,
            "--token-command",
            "echo secret-token:myauthtoken",
            "--root",
            "/myroot",
            "--group",
            "someothergroup",
        ]
        if use_alternate_endpoint:
            args += ["--api-url", f"http://{ALTERNATE_DOMAIN}:5000/{provider}"]

        if not configtype_default:
            args += ["--format", configtype]
        cmd = grm(args)

    if use_alternate_endpoint and provider == "github":
        assert cmd.returncode != 0
        assert "overriding is not supported for github" in cmd.stderr.lower()
        return
    assert cmd.returncode == 0
    assert len(cmd.stderr) == 0

    if configtype_default or configtype == "toml":
        output = toml.loads(cmd.stdout)
    elif configtype == "yaml":
        output = yaml.safe_load(cmd.stdout)
    else:
        raise NotImplementedError()

    assert isinstance(output, dict)
    assert set(output.keys()) == {"trees"}
    assert isinstance(output["trees"], list)
    assert len(output["trees"]) == 0


@pytest.mark.parametrize("provider", PROVIDERS)
@pytest.mark.parametrize("configtype_default", [False])
@pytest.mark.parametrize("configtype", ["toml", "yaml"])
@pytest.mark.parametrize("worktree_default", [True, False])
@pytest.mark.parametrize("worktree", [True, False])
@pytest.mark.parametrize("force_ssh", [True, False])
@pytest.mark.parametrize("use_alternate_endpoint", [True, False])
@pytest.mark.parametrize("use_config", [True, False])
def test_repos_find_remote_group(
    provider,
    configtype,
    configtype_default,
    worktree,
    worktree_default,
    force_ssh,
    use_alternate_endpoint,
    use_config,
):
    if use_config:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                cfg = f"""
                provider = "{provider}"
                token_command = "echo secret-token:myauthtoken"
                root = "/myroot"
                """

                if not worktree_default:
                    cfg += f"worktree = {str(worktree).lower()}\n"
                if force_ssh:
                    cfg += "force_ssh = true\n"
                if use_alternate_endpoint:
                    cfg += f'api_url = "http://{ALTERNATE_DOMAIN}:5000/{provider}"\n'
                cfg += """
                    [filters]
                    groups = ["mygroup1"]\n
                """

                f.write(cfg)

            args = ["repos", "find", "config", "--config", config.name]
            if not configtype_default:
                args += ["--format", configtype]
            cmd = grm(args)
    else:
        args = [
            "repos",
            "find",
            "remote",
            "--provider",
            provider,
            "--token-command",
            "echo secret-token:myauthtoken",
            "--root",
            "/myroot",
            "--group",
            "mygroup1",
        ]
        if not worktree_default:
            args += ["--worktree", str(worktree).lower()]
        if force_ssh:
            args += ["--force-ssh"]
        if use_alternate_endpoint:
            args += ["--api-url", f"http://{ALTERNATE_DOMAIN}:5000/{provider}"]

        if not configtype_default:
            args += ["--format", configtype]
        cmd = grm(args)
    if use_alternate_endpoint and provider == "github":
        assert cmd.returncode != 0
        assert "overriding is not supported for github" in cmd.stderr.lower()
        return
    assert cmd.returncode == 0
    assert len(cmd.stderr) == 0

    if configtype_default or configtype == "toml":
        output = toml.loads(cmd.stdout)
    elif configtype == "yaml":
        output = yaml.safe_load(cmd.stdout)
    else:
        raise NotImplementedError()

    assert isinstance(output, dict)
    assert set(output.keys()) == {"trees"}
    assert isinstance(output["trees"], list)
    assert len(output["trees"]) == 1

    assert set(output["trees"][0].keys()) == {"root", "repos"}
    assert isinstance(output["trees"][0]["repos"], list)
    assert len(output["trees"][0]["repos"]) == 5

    for i in range(1, 6):
        repo = [r for r in output["trees"][0]["repos"] if r["name"] == f"myproject{i}"][
            0
        ]
        assert repo["worktree_setup"] is (not worktree_default and worktree)
        assert isinstance(repo["remotes"], list)
        assert len(repo["remotes"]) == 1
        if force_ssh or i == 1:
            assert repo["remotes"][0]["name"] == "origin"
            assert (
                repo["remotes"][0]["url"]
                == f"ssh://git@example.com/mygroup1/myproject{i}.git"
            )
            assert repo["remotes"][0]["type"] == "ssh"
        else:
            assert repo["remotes"][0]["name"] == "origin"
            assert (
                repo["remotes"][0]["url"]
                == f"https://example.com/mygroup1/myproject{i}.git"
            )
            assert repo["remotes"][0]["type"] == "https"


@pytest.mark.parametrize("provider", PROVIDERS)
@pytest.mark.parametrize("configtype_default", [False])
@pytest.mark.parametrize("configtype", ["toml", "yaml"])
@pytest.mark.parametrize("worktree_default", [True, False])
@pytest.mark.parametrize("worktree", [True, False])
@pytest.mark.parametrize("use_owner", [True, False])
@pytest.mark.parametrize("force_ssh", [True, False])
@pytest.mark.parametrize("use_alternate_endpoint", [True, False])
@pytest.mark.parametrize("use_config", [True, False])
def test_repos_find_remote_user_and_group(
    provider,
    configtype,
    configtype_default,
    worktree,
    worktree_default,
    use_owner,
    force_ssh,
    use_alternate_endpoint,
    use_config,
):
    if use_config:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                cfg = f"""
                provider = "{provider}"
                token_command = "echo secret-token:myauthtoken"
                root = "/myroot"
                """

                if not worktree_default:
                    cfg += f"worktree = {str(worktree).lower()}\n"
                if force_ssh:
                    cfg += "force_ssh = true\n"
                if use_alternate_endpoint:
                    cfg += f'api_url = "http://{ALTERNATE_DOMAIN}:5000/{provider}"\n'
                cfg += """
                    [filters]
                    groups = ["mygroup1"]\n
                """

                if use_owner:
                    cfg += "owner = true\n"
                else:
                    cfg += 'users = ["myuser1"]\n'

                f.write(cfg)

            args = ["repos", "find", "config", "--config", config.name]
            if not configtype_default:
                args += ["--format", configtype]
            cmd = grm(args)
    else:
        args = [
            "repos",
            "find",
            "remote",
            "--provider",
            provider,
            "--token-command",
            "echo secret-token:myauthtoken",
            "--root",
            "/myroot",
            "--group",
            "mygroup1",
        ]
        if use_owner:
            args += ["--owner"]
        else:
            args += ["--user", "myuser1"]
        if not worktree_default:
            args += ["--worktree", str(worktree).lower()]
        if force_ssh:
            args += ["--force-ssh"]
        if use_alternate_endpoint:
            args += ["--api-url", f"http://{ALTERNATE_DOMAIN}:5000/{provider}"]

        if not configtype_default:
            args += ["--format", configtype]
        cmd = grm(args)
    if use_alternate_endpoint and provider == "github":
        assert cmd.returncode != 0
        assert "overriding is not supported for github" in cmd.stderr.lower()
        return
    assert cmd.returncode == 0
    assert len(cmd.stderr) == 0

    if configtype_default or configtype == "toml":
        output = toml.loads(cmd.stdout)
    elif configtype == "yaml":
        output = yaml.safe_load(cmd.stdout)
    else:
        raise NotImplementedError()

    assert isinstance(output, dict)
    assert set(output.keys()) == {"trees"}
    assert isinstance(output["trees"], list)
    assert len(output["trees"]) == 2

    user_namespace = [t for t in output["trees"] if t["root"] == "/myroot/myuser1"][0]

    assert set(user_namespace.keys()) == {"root", "repos"}
    assert isinstance(user_namespace["repos"], list)
    assert len(user_namespace["repos"]) == 5

    for i in range(1, 6):
        repo = [r for r in user_namespace["repos"] if r["name"] == f"myproject{i}"][0]
        assert repo["worktree_setup"] is (not worktree_default and worktree)
        assert isinstance(repo["remotes"], list)
        assert len(repo["remotes"]) == 1
        assert repo["remotes"][0]["name"] == "origin"
        if force_ssh or i == 1:
            assert (
                repo["remotes"][0]["url"]
                == f"ssh://git@example.com/myuser1/myproject{i}.git"
            )
            assert repo["remotes"][0]["type"] == "ssh"
        else:
            assert (
                repo["remotes"][0]["url"]
                == f"https://example.com/myuser1/myproject{i}.git"
            )
            assert repo["remotes"][0]["type"] == "https"

    group_namespace = [t for t in output["trees"] if t["root"] == "/myroot/mygroup1"][0]

    assert set(group_namespace.keys()) == {"root", "repos"}
    assert isinstance(group_namespace["repos"], list)
    assert len(group_namespace["repos"]) == 5

    for i in range(1, 6):
        repo = [r for r in group_namespace["repos"] if r["name"] == f"myproject{i}"][0]
        assert repo["worktree_setup"] is (not worktree_default and worktree)
        assert isinstance(repo["remotes"], list)
        assert len(repo["remotes"]) == 1
        assert repo["remotes"][0]["name"] == "origin"
        if force_ssh or i == 1:
            assert (
                repo["remotes"][0]["url"]
                == f"ssh://git@example.com/mygroup1/myproject{i}.git"
            )
            assert repo["remotes"][0]["type"] == "ssh"
        else:
            assert (
                repo["remotes"][0]["url"]
                == f"https://example.com/mygroup1/myproject{i}.git"
            )
            assert repo["remotes"][0]["type"] == "https"


@pytest.mark.parametrize("provider", PROVIDERS)
@pytest.mark.parametrize("configtype_default", [False])
@pytest.mark.parametrize("configtype", ["toml", "yaml"])
@pytest.mark.parametrize("worktree_default", [True, False])
@pytest.mark.parametrize("worktree", [True, False])
@pytest.mark.parametrize("with_user_filter", [True, False])
@pytest.mark.parametrize("with_group_filter", [True, False])
@pytest.mark.parametrize("force_ssh", [True, False])
@pytest.mark.parametrize("use_alternate_endpoint", [True, False])
@pytest.mark.parametrize("use_config", [True, False])
def test_repos_find_remote_owner(
    provider,
    configtype,
    configtype_default,
    worktree,
    worktree_default,
    with_user_filter,
    with_group_filter,
    force_ssh,
    use_alternate_endpoint,
    use_config,
):
    if use_config:
        with tempfile.NamedTemporaryFile() as config:
            with open(config.name, "w") as f:
                cfg = f"""
                provider = "{provider}"
                token_command = "echo secret-token:myauthtoken"
                root = "/myroot"
                """

                if not worktree_default:
                    cfg += f"worktree = {str(worktree).lower()}\n"
                if force_ssh:
                    cfg += "force_ssh = true\n"
                if use_alternate_endpoint:
                    cfg += f'api_url = "http://{ALTERNATE_DOMAIN}:5000/{provider}"\n'
                cfg += """
                    [filters]
                    access = true\n
                """

                if with_user_filter:
                    cfg += 'users = ["myuser1"]\n'
                if with_group_filter:
                    cfg += 'groups = ["mygroup1"]\n'

                f.write(cfg)

            args = ["repos", "find", "config", "--config", config.name]
            if not configtype_default:
                args += ["--format", configtype]
            cmd = grm(args)

    else:
        args = [
            "repos",
            "find",
            "remote",
            "--provider",
            provider,
            "--token-command",
            "echo secret-token:myauthtoken",
            "--root",
            "/myroot",
            "--access",
        ]
        if not worktree_default:
            args += ["--worktree", str(worktree).lower()]
        if with_user_filter:
            args += ["--user", "myuser1"]
        if with_group_filter:
            args += ["--group", "mygroup1"]
        if force_ssh:
            args += ["--force-ssh"]
        if use_alternate_endpoint:
            args += ["--api-url", f"http://{ALTERNATE_DOMAIN}:5000/{provider}"]

        if not configtype_default:
            args += ["--format", configtype]
        cmd = grm(args)
    if use_alternate_endpoint and provider == "github":
        assert cmd.returncode != 0
        assert "overriding is not supported for github" in cmd.stderr.lower()
        return
    assert cmd.returncode == 0
    assert len(cmd.stderr) == 0

    if configtype_default or configtype == "toml":
        output = toml.loads(cmd.stdout)
    elif configtype == "yaml":
        output = yaml.safe_load(cmd.stdout)
    else:
        raise NotImplementedError()

    assert isinstance(output, dict)
    assert set(output.keys()) == {"trees"}
    assert isinstance(output["trees"], list)
    assert len(output["trees"]) == 4

    user_namespace_1 = [t for t in output["trees"] if t["root"] == "/myroot/myuser1"][0]

    assert set(user_namespace_1.keys()) == {"root", "repos"}
    assert isinstance(user_namespace_1["repos"], list)

    if with_user_filter:
        assert len(user_namespace_1["repos"]) == 5

        for i in range(1, 6):
            repo = [
                r for r in user_namespace_1["repos"] if r["name"] == f"myproject{i}"
            ][0]
            assert repo["worktree_setup"] is (not worktree_default and worktree)
            assert isinstance(repo["remotes"], list)
            assert len(repo["remotes"]) == 1
            assert repo["remotes"][0]["name"] == "origin"
            if force_ssh or i == 1:
                assert (
                    repo["remotes"][0]["url"]
                    == f"ssh://git@example.com/myuser1/myproject{i}.git"
                )
                assert repo["remotes"][0]["type"] == "ssh"
            else:
                assert (
                    repo["remotes"][0]["url"]
                    == f"https://example.com/myuser1/myproject{i}.git"
                )
                assert repo["remotes"][0]["type"] == "https"
    else:
        assert len(user_namespace_1["repos"]) == 2

        for i in range(1, 3):
            repo = [
                r for r in user_namespace_1["repos"] if r["name"] == f"myproject{i}"
            ][0]
            assert repo["worktree_setup"] is (not worktree_default and worktree)
            assert isinstance(repo["remotes"], list)
            assert len(repo["remotes"]) == 1
            assert repo["remotes"][0]["name"] == "origin"
            if force_ssh or i == 1:
                assert (
                    repo["remotes"][0]["url"]
                    == f"ssh://git@example.com/myuser1/myproject{i}.git"
                )
                assert repo["remotes"][0]["type"] == "ssh"
            else:
                assert (
                    repo["remotes"][0]["url"]
                    == f"https://example.com/myuser1/myproject{i}.git"
                )
                assert repo["remotes"][0]["type"] == "https"

    user_namespace_2 = [t for t in output["trees"] if t["root"] == "/myroot/myuser2"][0]

    assert set(user_namespace_2.keys()) == {"root", "repos"}
    assert isinstance(user_namespace_2["repos"], list)
    assert len(user_namespace_2["repos"]) == 1

    repo = user_namespace_2["repos"][0]
    assert repo["worktree_setup"] is (not worktree_default and worktree)
    assert isinstance(repo["remotes"], list)
    assert len(repo["remotes"]) == 1
    assert repo["remotes"][0]["name"] == "origin"
    if force_ssh:
        assert (
            repo["remotes"][0]["url"] == "ssh://git@example.com/myuser2/myproject3.git"
        )
        assert repo["remotes"][0]["type"] == "ssh"
    else:
        assert repo["remotes"][0]["url"] == "https://example.com/myuser2/myproject3.git"
        assert repo["remotes"][0]["type"] == "https"

    group_namespace_1 = [t for t in output["trees"] if t["root"] == "/myroot/mygroup1"][
        0
    ]

    assert set(group_namespace_1.keys()) == {"root", "repos"}
    assert isinstance(group_namespace_1["repos"], list)

    if with_group_filter:
        assert len(group_namespace_1["repos"]) == 5

        for i in range(1, 6):
            repo = [
                r for r in group_namespace_1["repos"] if r["name"] == f"myproject{i}"
            ][0]
            assert repo["worktree_setup"] is (not worktree_default and worktree)
            assert isinstance(repo["remotes"], list)
            assert len(repo["remotes"]) == 1
            assert repo["remotes"][0]["name"] == "origin"
            if force_ssh or i == 1:
                assert (
                    repo["remotes"][0]["url"]
                    == f"ssh://git@example.com/mygroup1/myproject{i}.git"
                )
                assert repo["remotes"][0]["type"] == "ssh"
            else:
                assert (
                    repo["remotes"][0]["url"]
                    == f"https://example.com/mygroup1/myproject{i}.git"
                )
                assert repo["remotes"][0]["type"] == "https"
    else:
        assert len(group_namespace_1["repos"]) == 1

        repo = group_namespace_1["repos"][0]
        assert repo["worktree_setup"] is (not worktree_default and worktree)
        assert isinstance(repo["remotes"], list)
        assert len(repo["remotes"]) == 1
        assert repo["remotes"][0]["name"] == "origin"
        if force_ssh:
            assert (
                repo["remotes"][0]["url"]
                == "ssh://git@example.com/mygroup1/myproject4.git"
            )
            assert repo["remotes"][0]["type"] == "ssh"
        else:
            assert (
                repo["remotes"][0]["url"]
                == "https://example.com/mygroup1/myproject4.git"
            )
            assert repo["remotes"][0]["type"] == "https"

    group_namespace_2 = [t for t in output["trees"] if t["root"] == "/myroot/mygroup2"][
        0
    ]

    assert set(group_namespace_2.keys()) == {"root", "repos"}
    assert isinstance(group_namespace_2["repos"], list)
    assert len(group_namespace_2["repos"]) == 1

    repo = group_namespace_2["repos"][0]
    assert repo["worktree_setup"] is (not worktree_default and worktree)
    assert isinstance(repo["remotes"], list)
    assert len(repo["remotes"]) == 1
    assert repo["remotes"][0]["name"] == "origin"
    if force_ssh:
        assert (
            repo["remotes"][0]["url"] == "ssh://git@example.com/mygroup2/myproject5.git"
        )
        assert repo["remotes"][0]["type"] == "ssh"
    else:
        assert (
            repo["remotes"][0]["url"] == "https://example.com/mygroup2/myproject5.git"
        )
        assert repo["remotes"][0]["type"] == "https"
