# Releases

To make a release, make sure you are on a clean `develop` branch, sync your
remotes and then run `./release (major|minor|patch)`. It will handle a
git-flow-y release, meaning that it will perform a merge from `develop` to
`master`, create a git tag, sync all remotes and run `cargo publish`.

The release script will also run `just check` to make sure that nothing it
broken.

As GRM is still `v0.x`, there is not much consideration for backwards
compatibility. Generally, update the patch version for small stuff and the minor
version for bigger / backwards incompatible changes.

Generally, it's good to regularly release a new patch release with [updated
dependencies](./dependency_updates.md). As `./release.sh patch` is exposed as a
Justfile target (`release-patch`), it's possible to do both in one step:

```bash
$ just update-dependencies release-patch
```

## Release notes

There are currently no release notes. Things are changing quite quickly and
there is simply no need for a record of changes (except the git history of
course).
