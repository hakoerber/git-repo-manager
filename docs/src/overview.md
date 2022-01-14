# Overview

Welcome! This is the documentation for [Git Repo
Manager](https://github.com/hakoerber/git-repo-manager/) (GRM for short), a
tool that helps you manage git repositories.

GRM helps you manage git repositories in a declarative way. Configure your
repositories in a TOML or YAML file, GRM does the rest. Take a look at [the
example
configuration](https://github.com/hakoerber/git-repo-manager/blob/master/example.config.toml)
to get a feel for the way you configure your repositories. See the [repository
tree chapter](./repos.md) for details.

GRM also provides some tooling to work with single git repositories using
`git-worktree`. See [the worktree chapter](./worktree.md) for more details.

## Why use GRM?

If you're working with a lot of git repositories, GRM can help you to manage them
in an easy way:

* You want to easily clone many repositories to a new machine.
* You want to change remotes for multiple repositories (e.g. because your GitLab
  domain changed).
* You want to get an overview over all repositories you have, and check whether
  you forgot to commit or push something.

If you want to work with [git worktrees](https://git-scm.com/docs/git-worktree)
in a streamlined, easy way, GRM provides you with an opinionated workflow. It's
especially helpful when the following describes you:

* You're juggling a lot of git branches, switching between them a lot.
* When switching branches, you'd like to just leave your work as-is, without
  using the stash or temporary commits.
