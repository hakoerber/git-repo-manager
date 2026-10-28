"""Config exclusions through the real CLI and a loopback-only provider API."""

import json
import re
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlencode, urlsplit

import git
import pytest
import toml
import yaml
from helpers import checksum_directory, grm


@pytest.fixture(scope="module")
def provider_api():
    requests = []
    projects = [
        ("team", "skip", False, False),
        ("team", "archived", True, False),
        ("team", "archived-fork", True, True),
        ("team", "keep", False, False),
        ("team", "fork", False, True),
        ("other", "skip", False, False),
        ("team", "skip-extra", False, False),
    ]

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_GET(self):
            url = urlsplit(self.path)
            query = parse_qs(url.query)
            requests.append(self.path)
            if url.path in ("/github/user", "/gitlab/api/v4/user"):
                self.respond({"login": "team", "username": "team"})
                return
            if url.path not in (
                "/github/user/repos",
                "/github/users/team/repos",
                "/github/orgs/team/repos",
                "/gitlab/api/v4/projects",
                "/gitlab/api/v4/users/team/projects",
                "/gitlab/api/v4/groups/team/projects",
            ):
                self.respond({"message": "unexpected endpoint"}, status=404)
                return

            github = url.path.startswith("/github/")
            repos = []
            for group, name, archived, fork in projects:
                if query.get("archived") == ["false"] and archived:
                    continue
                namespace = group if github else f"{group}/sub"
                fullname = f"{namespace}/{name}"
                if github:
                    repo = {
                        "name": name,
                        "full_name": fullname,
                        "clone_url": f"https://example.com/{fullname}.git",
                        "ssh_url": f"git@example.com:{fullname}.git",
                        "private": False,
                        "fork": fork,
                        "archived": archived,
                    }
                else:
                    repo = {
                        "path": name,
                        "path_with_namespace": fullname,
                        "http_url_to_repo": f"https://example.com/{fullname}.git",
                        "ssh_url_to_repo": f"git@example.com:{fullname}.git",
                        "visibility": "public",
                        "forked_from_project": {} if fork else None,
                        "archived": archived,
                    }
                repos.append(repo)
            page = query.get("page", ["1"])[0]
            link = None
            if page == "1":
                query["page"] = ["2"]
                next_page = urlencode(query, doseq=True)
                link = (
                    f"<http://127.0.0.1:{self.server.server_port}"
                    f'{url.path}?{next_page}>; rel="next"'
                )
            self.respond(repos[:3] if page == "1" else repos[3:], link=link)

        def respond(self, data, status=200, link=None):
            body = json.dumps(data).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            if link:
                self.send_header("Link", link)
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}", requests, projects
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def write_config(tmp_path, provider, api_url, filters, config_format):
    config = {
        "provider": provider,
        "api_url": f"{api_url}/{provider}",
        "token_command": "echo secret-token:fixture-only",
        "root": str(tmp_path / "repos"),
    }
    if filters is not None:
        config["filters"] = filters
    path = tmp_path / f"config.{config_format}"
    path.write_text(
        toml.dumps(config) if config_format == "toml" else yaml.safe_dump(config)
    )
    return path


@pytest.mark.parametrize("provider", ["github", "gitlab"])
@pytest.mark.parametrize("config_format", ["toml", "yaml"])
@pytest.mark.parametrize(
    "selection", ["access", "owner", "users", "groups", "combined"]
)
@pytest.mark.parametrize(
    "scenario",
    [
        "defaults",
        "disabled",
        "archive",
        "exclude",
        "combined",
        "all",
        "wildcard-org",
        "wildcard-prefix",
        "wildcard-suffix",
        "wildcard-combined",
        "wildcard-all",
    ],
)
def test_provider_exclusions(
    tmp_path, provider_api, provider, config_format, selection, scenario
):
    api_url, requests, projects = provider_api
    requests.clear()
    prefix = "team" if provider == "github" else "team/sub"
    other = "other" if provider == "github" else "other/sub"
    selections = {
        "access": {"access": True},
        "owner": {"owner": True},
        "users": {"users": ["team"]},
        "groups": {"groups": ["team"]},
        "combined": {
            "access": True,
            "owner": True,
            "users": ["team"],
            "groups": ["team"],
        },
    }
    exclusions = {
        "defaults": {},
        "disabled": {"ignore_archived": False, "exclude": []},
        "archive": {"ignore_archived": True},
        "exclude": {"exclude": [f"{prefix}/skip"]},
        "combined": {
            "ignore_archived": True,
            "exclude": [f"{prefix}/skip"],
            "fork": False,
        },
        "all": {
            "exclude": [
                f"{prefix if group == 'team' else other}/{name}"
                for group, name, _, _ in projects
            ]
        },
        "wildcard-org": {"exclude": ["team/*"]},
        "wildcard-prefix": {"exclude": [f"{prefix}/skip*"]},
        "wildcard-suffix": {"exclude": ["*/skip"]},
        "wildcard-combined": {
            "ignore_archived": True,
            "fork": False,
            "exclude": [f"{prefix}/skip*", f"{other}/skip"],
        },
        "wildcard-all": {"exclude": ["*"]},
    }
    filters = {**selections[selection], **exclusions[scenario]}
    config = write_config(tmp_path, provider, api_url, filters, config_format)
    cmd = grm(["repos", "find", "config", "--config", str(config), "--format", "yaml"])
    assert cmd.returncode == 0, cmd.stderr
    result = yaml.safe_load(cmd.stdout)
    actual = [
        str(Path(tree["root"]).relative_to(tmp_path / "repos") / repo["name"])
        for tree in result["trees"]
        for repo in tree["repos"]
    ]
    expected = set()
    for group, name, archived, fork in projects:
        fullname = f"{prefix if group == 'team' else other}/{name}"
        if archived and (
            filters.get("ignore_archived", False)
            or (provider == "gitlab" and selection == "groups")
        ):
            continue
        if fork and not filters.get("fork", True):
            continue
        if any(
            re.fullmatch(".*".join(map(re.escape, pattern.split("*"))), fullname)
            for pattern in filters.get("exclude", [])
        ):
            continue
        expected.add(fullname)
    assert set(actual) == expected
    assert len(actual) == len(expected), "combined selections must not duplicate repos"
    assert any("page=2" in request for request in requests)
    assert not (tmp_path / "repos").exists(), "find must not create local clones"


@pytest.mark.parametrize("provider", ["github", "gitlab"])
@pytest.mark.parametrize(
    "filters", [None, {}, {"ignore_archived": True, "exclude": []}]
)
def test_exclusions_do_not_select_repos(tmp_path, provider_api, provider, filters):
    api_url, requests, _ = provider_api
    requests.clear()
    config = write_config(tmp_path, provider, api_url, filters, "toml")
    cmd = grm(["repos", "find", "config", "--config", str(config)])
    assert cmd.returncode == 0, cmd.stderr
    assert toml.loads(cmd.stdout) == {"trees": []}
    assert not requests


@pytest.mark.parametrize("provider", ["github", "gitlab"])
@pytest.mark.parametrize("wildcards", [False, True])
def test_sync_leaves_excluded_local_clones_unchanged(
    tmp_path, provider_api, provider, wildcards
):
    api_url, _, _ = provider_api
    prefix = "team" if provider == "github" else "team/sub"
    other = "other" if provider == "github" else "other/sub"
    root = tmp_path / "repos" / prefix
    for name in ["keep", "skip", "archived", "fork"]:
        path = root / name
        git.Repo.init(path, mkdir=True)
        (path / "local-changes").write_text("Do not touch this file.\n")
    untouched = {
        name: checksum_directory(str(root / name))
        for name in ["skip", "archived", "fork"]
    }
    config = write_config(
        tmp_path,
        provider,
        api_url,
        {
            "access": True,
            "ignore_archived": True,
            "fork": False,
            "exclude": (
                [f"{prefix}/skip*", f"{other}/*"]
                if wildcards
                else [f"{prefix}/skip", f"{other}/skip", f"{prefix}/skip-extra"]
            ),
        },
        "toml",
    )
    cmd = grm(["repos", "sync", "config", "--config", str(config)])
    assert cmd.returncode == 0, cmd.stderr
    for name, checksum in untouched.items():
        assert checksum_directory(str(root / name)) == checksum
    assert git.Repo(root / "keep").remotes.origin.url.endswith(f"{prefix}/keep.git")
    assert not (root / "archived-fork").exists()
    assert not (root / "skip-extra").exists()
    assert not (tmp_path / "repos" / other).exists()
