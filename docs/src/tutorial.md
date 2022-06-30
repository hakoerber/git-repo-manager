# Tutorial

Here, you'll find a quick overview over the most common functionality of GRM.

## Managing existing repositories

Let's say you have your git repositories at `~/code`. To start managing them via
GRM, first create a configuration:

```bash
grm repos find local ~/code --format yaml > ~/code/config.yml
```

The result may look something like this:

```yaml
---
trees:
  - root: ~/code
    repos:
      - name: git-repo-manager
        worktree_setup: true
        remotes:
          - name: origin
            url: "https://github.com/hakoerber/git-repo-manager.git"
            type: https
```

To apply the configuration and check whether all repositories are in sync, run
the following:

```bash
$ grm repos sync config --config ~/code/config.yml
[✔] git-repo-manager: OK
```

Well, obiously there are no changes. To check how changes would be applied,
let's change the name of the remote (currently `origin`):

```bash
$ sed -i 's/name: origin/name: github/' ~/code/config.yml
$ grm repos sync config --config ~/code/config.yml
[⚙] git-repo-manager: Setting up new remote "github" to "https://github.com/hakoerber/git-repo-manager.git"
[⚙] git-repo-manager: Deleting remote "origin"
[✔] git-repo-manager: OK
```

GRM replaced the `origin` remote with `github`.

The configuration (`~/code/config.yml` in this example) would usually be
something you'd track in git or synchronize between machines via some other
means. Then, on every machine, all your repositories are a single `grm repos
sync` away!

## Getting repositories from a forge

Let's say you have a bunch of repositories on GitHub and you'd like to clone
them all to your local machine.

To authenticate, you'll need to get a personal access token, as described in
[the forge documentation](./forge_integration.md#github). Let's assume you put
your token into `~/.github_token` (please don't if you're doing this "for
real"!)

Let's first see what kind of repos we can find:

```bash
$ grm repos sync remote --provider github --token-command "cat ~/.github_token" --root ~/code/github.com/ --format yaml
---
trees: []
$
```

Ummm, ok? No repos? This is because you have to *tell* GRM what to look for (if
you don't, GRM will just relax, as it's lazy).

There are different filters (see [the forge
documentation](./forge_integration.md#filters) for more info). In our case,
we'll just use the `--owner` filter to get all repos that belong to us:

```bash
$ grm repos find remote --provider github --token-command "cat ~/.github_token" --root ~/code/github.com/ --format yaml
---
trees:
  - root: ~/code/github.com
    repos:
      - name: git-repo-manager
        worktree_setup: false
        remotes:
          - name: origin
            url: "https://github.com/hakoerber/git-repo-manager.git"
            type: https
```

Nice! The format is the same as we got from `grm repos find local` above. So if
we wanted, we could save this file and use it with `grm repos sync config` as
above. But there is an even easier way: We can directly clone the repositories!

```bash
$ grm repos sync remote --provider github --token-command "cat ~/.github_token" --root ~/code/github.com/
[⚙] Cloning into "~/code/github.com/git-repo-manager" from "https://github.com/hakoerber/git-repo-manager.git"
[✔] git-repo-manager: Repository successfully cloned
[✔] git-repo-manager: OK
```

Nice! Just to make sure, let's run the same command again:

```bash
$ grm repos sync remote --provider github --token-command "cat ~/.github_token" --root ~/code/github.com/
[✔] git-repo-manager: OK
```

GRM saw that the repository is already there and did nothing (remember, it's
lazy).

## Using worktrees

Worktrees are something that make it easier to work with multiple branches at
the same time in a repository.  Let's say we wanted to hack on the codebase of
GRM:

```bash
$ cd ~/code/github.com/git-repo-manager
$ ls
.gitignore
Cargo.toml
...
```

Well, this is just a normal git repository. But let's try worktrees! First, we
have to convert the existing repository to use the special worktree setup. For
all worktree operations, we will use `grm worktree` (or `grm wt` for short):

```bash
$ grm wt convert
[✔] Conversion done
$ ls
$
```

So, the code is gone? Not really, there is just no active worktree right now. So
let's add one for `master`:


```bash
$ grm wt add master --track origin/master
[✔] Conversion done
$ ls
master
$ (cd ./master && git status)
On branch master
nothing to commit, working tree clean
```

Now, a single worktree is kind of pointless (if we only have one, we could also
just use the normal setup, without worktrees). So let's another one for
`develop`:

```bash
$ grm wt add develop --track origin/develop
[✔] Conversion done
$ ls
develop
master
$ (cd ./develop && git status)
On branch develop
nothing to commit, working tree clean
```

What's the point? The cool thing is that we can now start working in the
`develop` worktree, without affecting the `master` worktree at all. If you're
working on `develop` and want to quickly see what a certain file looks like in
`master`, just look inside `./master`, it's all there!

This becomes especially interesting when you have many feature branches and are
working on multiple features at the same time.

There are a lot of options that influence how worktrees are handled. Maybe you
want to automatically track `origin/master` when you add a worktree called
`master`?  Maybe you want your feature branches to have a prefix, so when you're
working on the `feature1` worktree, the remote branch will be
`origin/awesomefeatures/feature1`? Check out [the chapter on
worktrees](./worktrees.md) for all the things that are possible.
