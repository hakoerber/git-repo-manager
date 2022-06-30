# Git Worktrees

## Why?
The default workflow when using git is having your repository in a single
directory.  Then, you can check out a certain reference (usually a branch),
which will update the files in the directory to match the state of that
reference. Most of the time, this is exactly what you need and works perfectly.
But especially when you're working with branches a lot, you may notice that
there is a lot of work required to make everything run smoothly.

Maybe you have experienced the following: You're working on a feature branch.
Then, for some reason, you have to change branches (maybe to investigate some
issue).  But you get the following:

```
error: Your local changes to the following files would be overwritten by checkout
```

Now you can create a temporary commit or stash your changes. In any case, you
have some mental overhead before you can work on something else. Especially with
stashes, you'll have to remember to do a `git stash pop` before resuming your
work (I cannot count the number of times where I "rediscovered" some code hidden
in some old stash I forgot about). Also, conflicts on a `git stash pop` are just
horrible.

And even worse: If you're currently in the process of resolving merge conflicts
or an interactive rebase, there is just no way to "pause" this work to check out
a different branch.

Sometimes, it's crucial to have an unchanging state of your repository until
some long-running process finishes. I'm thinking of Ansible and Terraform runs.
I'd rather not change to a different branch while ansible or Terraform are
running as I have no idea how those tools would behave (and I'm not too eager to
find out).

In any case, Git Worktrees are here for the rescue:

## What are git worktrees?

[Git Worktrees](https://git-scm.com/docs/git-worktree) allow you to have
multiple independent checkouts of your repository on different directories. You
can have multiple directories that correspond to different references in your
repository.  Each worktree has it's independent working tree (duh) and index, so
there is no way to run into conflicts. Changing to a different branch is just a
`cd` away (if the worktree is already set up).

## Worktrees in GRM

GRM exposes an opinionated way to use worktrees in your repositories.
Opinionated, because there is a single invariant that makes reasoning about your
worktree setup quite easy:

**The branch inside the worktree is always the same as the directory name of the
worktree.**

In other words: If you're checking out branch `mybranch` into a new worktree,
the worktree directory will be named `mybranch`.

GRM can be used with both "normal" and worktree-enabled repositories. But note
that a single repository can be either the former or the latter. You'll have to
decide during the initial setup which way you want to go for that repository.

If you want to clone your repository in a worktree-enabled way, specify
`worktree_setup = true` for the repository in your `config.toml`:

```toml
[[trees.repos]]
name = "git-repo-manager"
worktree_setup = true
```

Now, when you run a `grm sync`, you'll notice that the directory of the
repository is empty! Well, not totally, there is a hidden directory called
`.git-main-working-tree`.  This is where the repository actually "lives" (it's a
bare checkout).

Note that there are few specific things you can configure for a certain
workspace.  This is all done in an optional `grm.toml` file right in the root of
the worktree. More on that later.


## Manual access

GRM isn't doing any magic, it's just git under the hood. If you need to have
access to the underlying git repository, you can always do this:

```
$ git --git-dir ./.git-main-working-tree [...]
```

This should never be required (whenever you have to do this, you can consider
this a bug in GRM and open an
[issue](https://github.com/hakoerber/git-repo-manager/issues/new), but it may
help in a pinch.

