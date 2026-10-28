"""Repository exclusion filters against the mocked provider APIs.

The mocks return myproject1 to myproject5. myproject4 is archived.
"""

import os
import tempfile

import git
import pytest
import toml
from helpers import checksum_directory, grm

PROVIDERS = ["github", "gitlab"]
TOKEN_COMMAND = "echo secret-token:myauthtoken"
ROOT = "/myroot"

SELECTIONS = {
    "access": {"access": True},
    "owner": {"owner": True},
    "users": {"users": ["myuser1"]},
    "groups": {"groups": ["mygroup1"]},
}

ACCESS_REPOS = [
    "mygroup1/myproject4",
    "mygroup2/myproject5",
    "myuser1/myproject1",
    "myuser1/myproject2",
    "myuser2/myproject3",
]

EXCLUSIONS = {
    "ignore_archived": True,
    "exclude_repos": ["MYUSER1/myproject1", "*/myproject5"],
}

EXPECTED_AFTER_EXCLUSIONS = {
    "access": ["myuser1/myproject2", "myuser2/myproject3"],
    "owner": ["myuser1/myproject2", "myuser1/myproject3"],
    "users": ["myuser1/myproject2", "myuser1/myproject3"],
    "groups": ["mygroup1/myproject1", "mygroup1/myproject2", "mygroup1/myproject3"],
    "combined": [
        "mygroup1/myproject1",
        "mygroup1/myproject2",
        "mygroup1/myproject3",
        "myuser1/myproject2",
        "myuser1/myproject3",
        "myuser2/myproject3",
    ],
}


def repo_paths(cmd):
    assert cmd.returncode == 0
    paths = []
    for tree in toml.loads(cmd.stdout)["trees"]:
        namespace = os.path.relpath(tree["root"], ROOT)
        paths += [f"{namespace}/{repo['name']}" for repo in tree["repos"]]
    return sorted(paths)


def run_with_config(content, action="find"):
    with tempfile.NamedTemporaryFile() as config:
        with open(config.name, "w") as f:
            f.write(content)
        return grm(["repos", action, "config", "--config", config.name])


def provider_config(provider, filters, root=ROOT):
    return toml.dumps(
        {
            "provider": provider,
            "token_command": TOKEN_COMMAND,
            "root": root,
            "filters": filters,
        }
    )


def find_config(provider, filters):
    cmd = run_with_config(provider_config(provider, filters))
    return cmd, repo_paths(cmd)


def find_remote(provider, args):
    cmd = grm(
        [
            "repos",
            "find",
            "remote",
            "--provider",
            provider,
            "--token-command",
            TOKEN_COMMAND,
            "--root",
            ROOT,
            *args,
        ]
    )
    return cmd, repo_paths(cmd)


@pytest.mark.parametrize("provider", PROVIDERS)
@pytest.mark.parametrize("selection", [*SELECTIONS, "combined"])
def test_exclusions_apply_to_every_selection(provider, selection):
    if selection == "combined":
        filters = {k: v for f in SELECTIONS.values() for k, v in f.items()}
    else:
        filters = SELECTIONS[selection]
    _, repos = find_config(provider, {**filters, **EXCLUSIONS})
    assert repos == EXPECTED_AFTER_EXCLUSIONS[selection]


@pytest.mark.parametrize("provider", PROVIDERS)
@pytest.mark.parametrize(
    "exclusions, expected",
    [
        ({}, ACCESS_REPOS),
        ({"ignore_archived": False, "exclude_repos": []}, ACCESS_REPOS),
        (
            {"ignore_archived": True},
            [
                "mygroup2/myproject5",
                "myuser1/myproject1",
                "myuser1/myproject2",
                "myuser2/myproject3",
            ],
        ),
        (
            {"exclude_repos": ["myuser2/myproject3"]},
            [
                "mygroup1/myproject4",
                "mygroup2/myproject5",
                "myuser1/myproject1",
                "myuser1/myproject2",
            ],
        ),
        (
            {"exclude_repos": ["MyUser2/MyProject3"]},
            [
                "mygroup1/myproject4",
                "mygroup2/myproject5",
                "myuser1/myproject1",
                "myuser1/myproject2",
            ],
        ),
        (
            {"exclude_repos": ["myuser1/*"]},
            ["mygroup1/myproject4", "mygroup2/myproject5", "myuser2/myproject3"],
        ),
        (
            {"exclude_repos": ["mygroup*", "*project1"]},
            ["myuser1/myproject2", "myuser2/myproject3"],
        ),
        ({"exclude_repos": ["*"]}, []),
    ],
    ids=[
        "defaults",
        "disabled",
        "archived",
        "exact",
        "exact-ignores-case",
        "wildcard-namespace",
        "wildcard-prefix-suffix",
        "wildcard-all",
    ],
)
def test_exclusion_patterns(provider, exclusions, expected):
    _, repos = find_config(provider, {"access": True, **exclusions})
    assert repos == expected


@pytest.mark.parametrize("provider", PROVIDERS)
def test_yaml_config_with_quoted_wildcards(provider):
    cmd = run_with_config(f"""
provider: {provider}
token_command: "{TOKEN_COMMAND}"
root: {ROOT}
filters:
  access: true
  exclude_repos:
    - "*/myproject5"
    - 'MYUSER1/*'
""")
    assert repo_paths(cmd) == ["mygroup1/myproject4", "myuser2/myproject3"]


@pytest.mark.parametrize("provider", PROVIDERS)
def test_find_remote_flags(provider):
    _, repos = find_remote(
        provider,
        [
            "--access",
            "--ignore-archived",
            "--exclude-repo",
            "MYUSER1/myproject1",
            "--exclude-repo",
            "*/myproject5",
        ],
    )
    assert repos == ["myuser1/myproject2", "myuser2/myproject3"]


@pytest.mark.parametrize("provider", PROVIDERS)
def test_sync_remote_flags(provider, tmp_path):
    # Without both flags, grm would try to clone the remaining repositories.
    cmd = grm(
        [
            "repos",
            "sync",
            "remote",
            "--provider",
            provider,
            "--token-command",
            TOKEN_COMMAND,
            "--root",
            str(tmp_path),
            "--access",
            "--ignore-archived",
            "--exclude-repo",
            "myuser*",
            "--exclude-repo",
            "mygroup2/*",
        ]
    )
    assert cmd.returncode == 0
    assert list(tmp_path.iterdir()) == []


@pytest.mark.parametrize("provider", PROVIDERS)
@pytest.mark.parametrize("use_config", [True, False])
def test_unmatched_exclusions_warn(provider, use_config):
    # mygroup1/myproject4 is archived and skipped anyway, but its entry matches.
    entries = ["nosuch/*", "myuser1/myproject1", "mygroup1/myproject4"]
    if use_config:
        cmd, repos = find_config(
            provider,
            {"access": True, "ignore_archived": True, "exclude_repos": entries},
        )
    else:
        args = ["--access", "--ignore-archived"]
        for entry in entries:
            args += ["--exclude-repo", entry]
        cmd, repos = find_remote(provider, args)
    assert repos == ["mygroup2/myproject5", "myuser1/myproject2", "myuser2/myproject3"]
    assert 'Exclusion "nosuch/*" did not match any repository' in cmd.stderr
    assert cmd.stderr.count("did not match") == 1


@pytest.mark.parametrize("provider", PROVIDERS)
def test_sync_leaves_excluded_clones_unchanged(provider, tmp_path):
    for path in [
        "myuser1/myproject1",
        "myuser2/myproject3",
        "mygroup1/myproject4",
        "mygroup2/myproject5",
    ]:
        git.Repo.init(tmp_path / path, mkdir=True)
        (tmp_path / path / "local-changes").write_text("Do not touch this file.\n")
    skipped = ["myuser1/myproject1", "mygroup1/myproject4", "mygroup2/myproject5"]
    checksums = {path: checksum_directory(str(tmp_path / path)) for path in skipped}

    filters = {
        "access": True,
        "ignore_archived": True,
        "exclude_repos": ["myuser1/*", "*/myproject5"],
    }
    cmd = run_with_config(
        provider_config(provider, filters, root=str(tmp_path)), action="sync"
    )

    assert cmd.returncode == 0
    for path, checksum in checksums.items():
        assert checksum_directory(str(tmp_path / path)) == checksum
    # Only myuser2/myproject3 remains. It exists locally, so sync adds the remote.
    origin = git.Repo(tmp_path / "myuser2/myproject3").remotes.origin
    assert origin.url.endswith("myuser2/myproject3.git")
    assert not (tmp_path / "myuser1/myproject2").exists()
