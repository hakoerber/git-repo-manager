# Working with Worktrees

## Creating a new worktree

To actually work, you'll first have to create a new worktree checkout. All
worktree-related commands are available as subcommands of `grm worktree` (or
`grm wt` for short):

```
$ grm wt add mybranch
[✔] Worktree mybranch created
```

You'll see that there is now a directory called `mybranch` that contains a
checkout of your repository, using the branch `mybranch`

```bash
$ cd ./mybranch && git status
On branch mybranch
nothing to commit, working tree clean
```

You can work in this repository as usual. Make changes, commit them, revert
them, whatever you're up to :)

Just note that you *should* not change the branch inside the worktree directory.
There is nothing preventing you from doing so, but you will notice that you'll
run into problems when trying to remove a worktree (more on that later). It may
also lead to confusing behavior, as there can be no two worktrees that have the
same branch checked out. So if you decide to use the worktree setup, go all in,
let `grm` manage your branches and bury `git branch` (and `git checkout -b`).

You will notice that there is no tracking branch set up for the new branch. You
can of course set up one manually after creating the worktree, but there is an
easier way, using the `--track` flag during creation. Let's create another
worktree. Go back to the root of the repository, and run:

```bash
$ grm wt add mybranch2 --track origin/mybranch2
[✔] Worktree mybranch2 created
```

You'll see that this branch is now tracking `mybranch` on the `origin` remote:

```bash
$ cd ./mybranch2 && git status
On branch mybranch

Your branch is up to date with 'origin/mybranch2'.
nothing to commit, working tree clean
```

The behavior of `--track` differs depending on the existence of the remote
branch:

* If the remote branch already exists, `grm` uses it as the base of the new
  local branch.
* If the remote branch does not exist (as in our example), `grm` will create a
  new remote tracking branch, using the default branch (either `main` or
  `master`) as the base

Often, you'll have a workflow that uses tracking branches by default. It would
be quite tedious to add `--track` every single time. Luckily, the `grm.toml`
file supports defaults for the tracking behavior. See this for an example:

```toml
[track]
default = true
default_remote = "origin"
```

This will set up a tracking branch on `origin` that has the same name as the
local branch.

Sometimes, you might want to have a certain prefix for all your tracking
branches.  Maybe to prevent collisions with other contributors. You can simply
set `default_remote_prefix` in `grm.toml`:

```toml
[track]
default = true
default_remote = "origin"
default_remote_prefix = "myname"
```

When using branch `my-feature-branch`, the remote tracking branch would be
`origin/myname/my-feature-branch` in this case.

Note that `--track` overrides any configuration in `grm.toml`. If you want to
disable tracking, use `--no-track`.

## Showing the status of your worktrees

There is a handy little command that will show your an overview over all
worktrees in a repository, including their status (i.e. changes files). Just run
the following in the root of your repository:

```
$ grm wt status
╭───────────┬────────┬──────────┬──────────────────╮
│ Worktree  ┆ Status ┆ Branch   ┆ Remote branch    │
╞═══════════╪════════╪══════════╪══════════════════╡
│ mybranch  ┆ ✔      ┆ mybranch ┆                  │
│ mybranch2 ┆ ✔      ┆ mybranch ┆ origin/mybranch2 │
╰───────────┴────────┴──────────┴──────────────────╯
```

The "Status" column would show any uncommitted changes (new / modified / deleted
files) and the "Remote branch" would show differences to the remote branch (e.g.
if there are new pushes to the remote branch that are not yet incorporated into
your local branch).


## Deleting worktrees

If you're done with your worktrees, use `grm wt delete` to delete them. Let's
start with `mybranch2`:

```
$ grm wt delete mybranch2
[✔] Worktree mybranch2 deleted
```

Easy. On to `mybranch`:

```
$ grm wt delete mybranch
[!] Changes in worktree: No remote tracking branch for branch mybranch found. Refusing to delete
```

Hmmm. `grm` tells you:

"Hey, there is no remote branch that you could have pushed your changes to. I'd
rather not delete work that you cannot recover."

Note that `grm` is very cautious here. As your repository will not be deleted,
you could still recover the commits via
[`git-reflog`](https://git-scm.com/docs/git-reflog).  But better safe than
sorry! Note that you'd get a similar error message if your worktree had any
uncommitted files, for the same reason. Now you can either commit & push your
changes, or your tell `grm` that you know what you're doing:

```
$ grm wt delete mybranch --force
[✔] Worktree mybranch deleted
```

If you just want to delete all worktrees that do not contain any changes, you
can also use the following:

```
$ grm wt clean
```

Note that this will not delete the default branch of the repository. It can of
course still be delete with `grm wt delete` if necessary.

### Converting an existing repository

It is possible to convert an existing directory to a worktree setup, using `grm
wt convert`. This command has to be run in the root of the repository you want
to convert:

```
$ grm wt convert
[✔] Conversion successful
```

This command will refuse to run if you have any changes in your repository.
Commit them and try again!

Afterwards, the directory is empty, as there are no worktrees checked out yet.
Now you can use the usual commands to set up worktrees.
