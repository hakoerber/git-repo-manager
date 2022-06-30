# Behavior Details

When working with worktrees and GRM, there is a lot going on under the hood.
Each time you create a new worktree, GRM has to figure out what commit to set
your new branch to and how to configure any potential remote branches.

To state again, the most important guideline is the following:

**The branch inside the worktree is always the same as the directory name of the
worktree.**

The second set of guidelines relates to the commit to check out, and the remote
branches to use:

* When a branch already exists, you will get a worktree for that branch
* Existing local branches are never changed
* Only do remote operations if specifically requested (via configuration file or
  command line parameters)
* When you specify `--track`, you will get that exact branch as the tracking
  branch
* When you specify `--no-track`, you will get no tracking branch

Apart from that, GRM tries to do The Right Thing<sup>TM</sup>. It should be as
little surprising as possible.

In 99% of the cases, you will not have to care about the details, as the normal
workflows are covered by the rules above. In case you want to know the exact
behavior "specification", take a look at the [module documentation for
`grm::worktree`](https://docs.rs/git-repo-manager/latest/grm/worktree/index.html).

If you think existing behavior is super-duper confusing and you have a better
idea, do not hesitate to open a GitHub issue to discuss this!
