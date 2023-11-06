# GRM — Git Repository Manager

GRM helps you manage git repositories in a declarative way. Configure your
repositories in a [TOML](https://toml.io/) or YAML file, GRM does the rest.

Also, GRM can be used to work with git worktrees in an opinionated,
straightforward fashion.

**Take a look at the [official documentation](https://hakoerber.github.io/git-repo-manager/)
for installation & quickstart.**

# Why?

I have a **lot** of repositories on my machines. My own stuff, forks, quick
clones of other's repositories, projects that never went anywhere ... In short,
I lost overview.

To sync these repositories between machines, I've been using Nextcloud. The thing
is, Nextcloud is not too happy about too many small files that change all the time,
like the files inside `.git`. Git also assumes that those files are updated as
atomically as possible. Nextcloud cannot guarantee that, so when I do a `git status`
during a sync, something blows up. And resolving these conflicts is just no fun ...

In the end, I think that git repos just don't belong into something like Nextcloud.
Git is already managing the content & versions, so there is no point in having
another tool do the same. But of course, setting up all those repositories from
scratch on a new machine is too much hassle. What if there was a way to clone all
those repos in a single command?

Also, I once transferred the domain of my personal git server. I updated a few
remotes manually, but I still stumble upon old, stale remotes in projects that
I haven't touched in a while. What if there was a way to update all those remotes
in once place?

This is how GRM came to be. I'm a fan of infrastructure-as-code, and GRM is a bit
like Terraform for your local git repositories. Write a config, run the tool, and
your repos are ready. The only thing that is tracked by git is the list of
repositories itself.

# Crates

* [`toml`](https://docs.rs/toml/) for the configuration file.
* [`serde`](https://docs.rs/serde/), together with
  [`serde_yaml`](https://docs.rs/serde_yaml/) and
  [`serde_json`](https://docs.rs/serde_json/). Because we're using Rust, after
  all.
* [`git2`](https://docs.rs/git2/), a safe wrapper around `libgit2`, for all git operations.
* [`clap`](https://docs.rs/clap/), [`console`](https://docs.rs/console/), [`comfy_table`](https://docs.rs/comfy-table/) and [`shellexpand`](https://docs.rs/shellexpand) for good UX.
* [`isahc`](https://docs.rs/isahc/) as the HTTP client for forge integrations.

# Links

* [crates.io](https://crates.io/crates/git-repo-manager)

# Mirrors

This repository can be found on multiple forges:

* https://github.com/hakoerber/git-repo-manager
* https://code.hkoerber.de/hannes/git-repo-manager/
* https://codeberg.org/hakoerber/git-repo-manager
* https://git.sr.ht/~hkoerber/git-repo-manager
