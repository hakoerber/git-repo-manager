# Managing Repositories

GRM helps you manage a bunch of git repositories easily. There are generally two
ways to go about that:

You can either manage a list of repositories in a TOML or YAML file, and use GRM
to sync the configuration with the state of the repository.

Or, you can pull repository information from a forge (e.g. GitHub, GitLab) and
clone the repositories.

There are also hybrid modes where you pull information from a forge and create a
configuration file that you can use later.
