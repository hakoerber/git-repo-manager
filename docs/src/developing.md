# Overview

GRM is still in very early development. I started GRM mainly to scratch my own
itches (and am heavily dogfooding it). If you have a new use case for GRM, go
for it!

## Contributing

To contribute, just fork the repo and create a pull request against `develop`.
If you plan bigger changes, please consider opening an issue first, so we can
discuss it.

If you want, add yourself to the `CONTRIBUTORS` file in your pull request.

## Branching strategy

The branching strategy is a simplified
[git-flow](https://nvie.com/posts/a-successful-git-branching-model/).

* `master` is the "production" branch. Each commit is a new release.
* `develop` is the branch where new stuff is coming in.
* feature branches branch off of `develop` and merge back into it.

Feature branches are not required, there are also changes happening directly on
`develop`.

## Required tooling

You will need the following tools:

* Rust (obviously) (easiest via `rustup`)
* Python3
* [`just`](https://github.com/casey/just), a command runner like `make`. See
  [here](https://github.com/casey/just#installation) for installation
  instructions (it's most likely just a simple `cargo install just`).
* Docker & docker-compose for the e2e tests
* `isort`, `black` and `shfmt` for formatting.
* `ruff` and `shellcheck` for linting.
* `mdbook` for the documentation

Here are the tools:

| Distribution  | Command                                                                                             |
| ------------- | --------------------------------------------------------------------------------------------------- |
| Arch Linux    | `pacman -S --needed python3 rustup just docker docker-compose python-black shfmt shellcheck mdbook` |
| Ubuntu/Debian | `apt-get install --no-install-recommends python3 docker.io docker-compose black shellcheck`         |

Note that you will have to install `just` and `mdbook` manually on Ubuntu (e.g.
via `cargo install just mdbook` if your rust build environment is set up
correctly). Same for `shfmt`, which may just be a `go install
mvdan.cc/sh/v3/cmd/shfmt@latest`, depending on your go build environment.

For details about rustup and the toolchains, see [the installation
section](./installation.md).
