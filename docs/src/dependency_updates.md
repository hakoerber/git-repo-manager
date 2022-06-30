# Dependency updates

Rust has the same problem as the node ecosystem, just a few magnitudes smaller:
Dependency sprawl. GRM has a dozen direct dependencies, but over 150 transitive
ones.

To keep them up to date, there is a script:
`depcheck/update-cargo-dependencies.py`. It updates direct dependencies to the
latest stable version and updates transitive dependencies where possible. To run
it, use `just update-dependencies`, which will create commits for each update.
