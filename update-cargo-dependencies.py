#!/usr/bin/env python3

import subprocess

import tomlkit

with open("./Cargo.toml", "r") as cargo_config:
    cargo = tomlkit.parse(cargo_config.read())

update_necessary = False

for tier in ["dependencies", "dev-dependencies"]:
    for name, dependency in cargo[tier].items():
        print(f"checking {name}")
        version = dependency["version"].lstrip("=")

        args = [
            "cargo",
            "upgrade",
            "--incompatible",
            "--pinned",
            "--package",
            name,
        ]
        args = [
            "cargo",
            "upgrade",
            "--incompatible",
            "--pinned",
            "--ignore-rust-version",
            "--recursive",
            "--package",
            name,
        ]
        subprocess.run(
            args,
            check=True,
        )

        with open("./Cargo.toml", "r") as cargo_config:
            cargo = tomlkit.parse(cargo_config.read())

            new_version = {dep: cfg for dep, cfg in cargo[tier].items() if dep == name}[
                name
            ]["version"].lstrip("=")

        if version != new_version:
            update_necessary = True

            message = f"dep: Update {name} to {new_version}"

            cmd = subprocess.run(
                [
                    "git",
                    "commit",
                    "--message",
                    message,
                    "./Cargo.lock",
                    "./Cargo.toml",
                ],
                check=True,
            )

        # If only Cargo.lock changed but not the version of the dependency itself,
        # some transitive dependencies were updated
        else:
            cmd = subprocess.run(
                [
                    "git",
                    "diff",
                    "--stat",
                    "--exit-code",
                    "./Cargo.lock",
                ],
            )

            if cmd.returncode == 1:
                message = f"dep: Update dependencies of {name}"

                cmd = subprocess.run(
                    [
                        "git",
                        "commit",
                        "--message",
                        message,
                        "./Cargo.lock",
                    ],
                    check=True,
                )

            # assert that Cargo.toml is not modified
            subprocess.run(
                [
                    "git",
                    "diff",
                    "--stat",
                    "--exit-code",
                    "./Cargo.toml",
                ],
                check=True,
            )


if update_necessary is False:
    print("Everything up to date")
