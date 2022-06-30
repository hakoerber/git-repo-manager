# Worktrees and Remotes

To fetch all remote references from all remotes in a worktree setup, you can use
the following command:

```
$ grm wt fetch
[✔] Fetched from all remotes
```

This is equivalent to running `git fetch --all` in any of the worktrees.

Often, you may want to pull all remote changes into your worktrees. For this,
use the `git pull` equivalent:

```
$ grm wt pull
[✔] master: Done
[✔] my-cool-branch: Done
```

This will refuse when there are local changes, or if the branch cannot be fast
forwarded. If you want to rebase your local branches, use the `--rebase` switch:

```
$ grm wt pull --rebase
[✔] master: Done
[✔] my-cool-branch: Done
```

As noted, this will fail if there are any local changes in your worktree. If you
want to stash these changes automatically before the pull (and unstash them
afterwards), use the `--stash` option.

This will rebase your changes onto the upstream branch. This is mainly helpful
for persistent branches that change on the remote side.

There is a similar rebase feature that rebases onto the **default** branch
instead:

```
$ grm wt rebase
[✔] master: Done
[✔] my-cool-branch: Done
```

This is super helpful for feature branches. If you want to incorporate changes
made on the remote branches, use `grm wt rebase` and all your branches will be
up to date. If you want to also update to remote tracking branches in one go,
use the `--pull` flag, and `--rebase` if you want to rebase instead of aborting
on non-fast-forwards:

```
$ grm wt rebase --pull --rebase
[✔] master: Done
[✔] my-cool-branch: Done
```

"So, what's the difference between `pull --rebase` and `rebase --pull`? Why the
hell is there a `--rebase` flag in the `rebase` command?"

Yes, it's kind of weird. Remember that `pull` only ever updates each worktree to
their remote branch, if possible. `rebase` rebases onto the **default** branch
instead. The switches to `rebase` are just convenience, so you do not have to
run two commands.

* `rebase --pull` is the same as `pull` && `rebase`
* `rebase --pull --rebase` is the same as `pull --rebase` && `rebase`

I understand that the UX is not the most intuitive. If you can think of an
improvement, please let me know (e.g. via an GitHub issue)!

As with `pull`, `rebase` will also refuse to run when there are changes in your
worktree. And you can also use the `--stash` option to stash/unstash changes
automatically.
