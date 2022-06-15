# Contributing

GRM is still in very early development. I started GRM mainly to scratch my own
itches (and am heavily dogfooding it). If you have a new use case for GRM, go
for it!

The branching strategy is a simplified
[git-flow](https://nvie.com/posts/a-successful-git-branching-model/).

* `master` is the "production" branch. Each commit is a new release.
* `develop` is the branch where new stuff is coming in.
* feature branches branch off of `develop` and merge back into it.

So to contribute, just fork the repo and create a pull request against
`develop`. If you plan bigger changes, please consider opening an issue first,
so we can discuss it.

If you want, add yourself to the `CONTRIBUTORS` file in your pull request.

## Code formatting

For Rust, just use `cargo fmt`. For Python, use
[black](https://github.com/psf/black). I'd rather not spend any effort in
configuring the formatters (not possible for black anyway). For shell scripts,
use [`shfmt`](https://github.com/mvdan/sh).

## Tooling

GRM uses [`just`](https://github.com/casey/just) as a command runner. See
[here](https://github.com/casey/just#installation) for installation
instructions (it's most likely just a simple `cargo install just`).

## Testing

There are two distinct test suites: One for unit test (`just test-unit`) and
integration tests (`just test-integration`) that is part of the rust crate, and
a separate e2e test suite in python (`just test-e2e`).

To run all tests, run `just test`.

When contributing, consider whether it makes sense to add tests which could
prevent regressions in the future. When fixing bugs, it makes sense to add
tests that expose the wrong behaviour beforehand.

## Documentation

The documentation lives in `docs` and uses
[mdBook](https://github.com/rust-lang/mdBook). Please document new user-facing
features here!

