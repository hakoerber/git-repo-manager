# Managing tree of git repositories

When managing multiple git repositories with GRM, you'll generally have a
configuration file containing information about all the repos you have. GRM then
makes sure that you repositories match that config. If they don't exist yet, it
will clone them. It will also make sure that all remotes are configured properly.

Let's try it out:

## Get the example configuration

```bash
$ curl --proto '=https' --tlsv1.2 -sSfO https://raw.githubusercontent.com/hakoerber/git-repo-manager/master/example.config.toml
```

Then, you're ready to run the first sync. This will clone all configured repositories
and set up the remotes.

```bash
$ grm repos sync --config example.config.toml
[⚙] Cloning into "/home/me/projects/git-repo-manager" from "https://code.hkoerber.de/hannes/git-repo-manager.git"
[✔] git-repo-manager: Repository successfully cloned
[⚙] git-repo-manager: Setting up new remote "github" to "https://github.com/hakoerber/git-repo-manager.git"
[✔] git-repo-manager: OK
[⚙] Cloning into "/home/me/projects/dotfiles" from "https://github.com/hakoerber/dotfiles.git"
[✔] dotfiles: Repository successfully cloned
[✔] dotfiles: OK
```

If you run it again, it will report no changes:

```
$ grm repos sync --config example.config.toml
[✔] git-repo-manager: OK
[✔] dotfiles: OK
```

### Generate your own configuration

Now, if you already have a few repositories, it would be quite laborious to write
a configuration from scratch. Luckily, GRM has a way to generate a configuration
from an existing file tree:

```bash
$ grm repos find ~/your/project/root > config.toml
```

This will detect all repositories and remotes and write them to `config.toml`.

### Show the state of your projects

```bash
$ grm repos status --config example.config.toml
╭──────────────────┬──────────┬────────┬───────────────────┬────────┬─────────╮
│ Repo             ┆ Worktree ┆ Status ┆ Branches          ┆ HEAD   ┆ Remotes │
╞══════════════════╪══════════╪════════╪═══════════════════╪════════╪═════════╡
│ git-repo-manager ┆          ┆ ✔      ┆ branch: master    ┆ master ┆ github  │
│                  ┆          ┆        ┆ <origin/master> ✔ ┆        ┆ origin  │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┤
│ dotfiles         ┆          ┆ ✔      ┆                   ┆ Empty  ┆ origin  │
╰──────────────────┴──────────┴────────┴───────────────────┴────────┴─────────╯
```

You can also use `status` without `--config` to check the repository you're currently
in:

```
$ cd ~/example-projects/dotfiles
$ grm repos status
╭──────────┬──────────┬────────┬──────────┬───────┬─────────╮
│ Repo     ┆ Worktree ┆ Status ┆ Branches ┆ HEAD  ┆ Remotes │
╞══════════╪══════════╪════════╪══════════╪═══════╪═════════╡
│ dotfiles ┆          ┆ ✔      ┆          ┆ Empty ┆ origin  │
╰──────────┴──────────┴────────┴──────────┴───────┴─────────╯
```

