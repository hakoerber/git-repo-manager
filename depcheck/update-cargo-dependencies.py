#!/usr/bin/env python3

import subprocess
import os
import json
import sys

import semver
import tomlkit

INDEX_DIR = "crates.io-index"

AUTOUPDATE_DISABLED = []

if os.path.exists(INDEX_DIR):
    subprocess.run(
        ["git", "fetch", "--depth=1", "origin"],
        cwd=INDEX_DIR,
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["git", "reset", "--hard", "origin/master"],
        cwd=INDEX_DIR,
        check=True,
        capture_output=True,
    )
else:
    subprocess.run(
        ["git", "clone", "--depth=1", "https://github.com/rust-lang/crates.io-index"],
        check=True,
        capture_output=False,  # to get some git output
    )

with open("../Cargo.toml", "r") as cargo_config:
    cargo = tomlkit.parse(cargo_config.read())

update_necessary = False

# This updates the crates.io index, see https://github.com/rust-lang/cargo/issues/3377
subprocess.run(
    ["cargo", "update", "--dry-run"],
    check=True,
    capture_output=False,  # to get some git output
)

for tier in ["dependencies", "dev-dependencies"]:
    for name, dependency in cargo[tier].items():
        version = dependency["version"].lstrip("=")
        if len(name) >= 4:
            info_file = f"{INDEX_DIR}/{name[0:2]}/{name[2:4]}/{name}"
        elif len(name) == 3:
            info_file = f"{INDEX_DIR}/3/{name[0]}/{name}"
        elif len(name) == 2:
            info_file = f"{INDEX_DIR}/2/{name}"
        elif len(name) == 1:
            info_file = f"{INDEX_DIR}/1/{name}"

        current_version = semver.VersionInfo.parse(version)

        latest_version = None
        for version_entry in open(info_file, "r").readlines():
            version = semver.VersionInfo.parse(json.loads(version_entry)["vers"])
            if latest_version is None or version > latest_version:
                if (
                    current_version.prerelease is None
                    and version.prerelease is not None
                ):
                    # skip prereleases, except when we are on a prerelease already
                    print(f"{name}: Skipping prerelease version {version}")
                    continue
                latest_version = version

        if latest_version != current_version:
            if name in AUTOUPDATE_DISABLED:
                print(
                    f"{name} {current_version}: There is a new version available "
                    f"({latest_version}, current {current_version}), but autoupdating "
                    f"is explictly disabled for {name}"
                )
                continue
            update_necessary = True
            if latest_version < current_version:
                print(
                    f"{name}: Your current version is newer than the newest version on crates.io, the hell?"
                )
            else:
                print(
                    f"{name}: New version found: {latest_version} (current {current_version})"
                )
                cargo[tier][name]["version"] = f"={str(latest_version)}"
            with open("../Cargo.toml", "w") as cargo_config:
                cargo_config.write(tomlkit.dumps(cargo))

            try:
                cmd = subprocess.run(
                    [
                        "cargo",
                        "update",
                        "-Z",
                        "no-index-update",
                        "--aggressive",
                        "--package",
                        name,
                    ],
                    check=True,
                    capture_output=True,
                    text=True,
                )
            except subprocess.CalledProcessError as e:
                print(e.stdout)
                print(e.stderr)
                raise

            message = f"dependencies: Update {name} to {latest_version}"
            subprocess.run(
                [
                    "git",
                    "commit",
                    "--message",
                    message,
                    "../Cargo.toml",
                    "../Cargo.lock",
                ],
                check=True,
                capture_output=True,
            )


# Note that we have to restart this lookup every time, as later packages can depend
# on former packages
while True:
    with open("../Cargo.lock", "r") as f:
        cargo_lock = tomlkit.parse(f.read())
    for package in cargo_lock["package"]:
        spec = f"{package['name']}:{package['version']}"
        try:
            cmd = subprocess.run(
                [
                    "cargo",
                    "update",
                    "-Z",
                    "no-index-update",
                    "--aggressive",
                    "--package",
                    spec,
                ],
                check=True,
                capture_output=True,
                text=True,
            )
        except subprocess.CalledProcessError as e:
            print(e.stdout)
            print(e.stderr)
            raise
        if len(cmd.stderr) != 0:
            update_necessary = True
            message = "Cargo.lock: {}".format(cmd.stderr.split("\n")[0].strip())
            print(message)
            cmd = subprocess.run(
                ["git", "commit", "--message", message, "../Cargo.lock"],
                check=True,
                capture_output=True,
            )
            break
    else:
        break

if update_necessary is False:
    print("Everything up to date")
