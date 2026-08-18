# FAQ

## Cloning via SSH fails, but `git clone` works

GRM talks to remotes via libgit2 instead of calling the `ssh` binary. This means
that it does not read your `~/.ssh/config`, so any keys configured via
`IdentityFile` are invisible to it.

For SSH remotes, GRM authenticates via the SSH agent that `SSH_AUTH_SOCK` points
to. Add your key to the agent like this:

```bash
$ eval "$(ssh-agent -s)"
$ ssh-add ~/.ssh/id_ed25519 # Replace `id_ed25519` by the ssh key you use for the git remote
```

Note that the agent is inherited via the environment, so it has to be set up in
the shell you are running GRM from.
