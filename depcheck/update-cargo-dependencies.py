#!/usr/bin/env python3

import subprocess
import os
import json
import sys

import semver
import tomlkit

INDEX_DIR = "crates.io-index"

AUTOUPDATE_DISABLED = [
    "clap",
]

if os.path.exists(INDEX_DIR):
    subprocess.run(
        ["git", "pull", "--depth=1", "origin"],
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
            if current_version.prerelease == "" and version.prerelease != "":
                # skip prereleases, except when we are on a prerelease already
                continue
            if latest_version is None or version > latest_version:
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

            message = f"dependencies: Update {name} to {latest_version}"
            subprocess.run(
                ["git", "commit", "--message", message, "../Cargo.toml"],
                check=True,
                capture_output=True
            )


if update_necessary is False:
    print("Everything up to date")
