# Forge Integrations

In addition to manging repositories locally, `grm` also integrates with source
code hosting platforms. Right now, the following platforms are supported:

* [GitHub](https://github.com/)
* [GitLab](https://gitlab.com/)

Imagine you are just starting out with `grm` and want to clone all your repositories
from GitHub. This is as simple as:

```bash
$ grm repos sync remote --provider github --owner --token-command "pass show github_grm_access_token" --path ~/projects
```

You will end up with your projects cloned into `~/projects/{your_github_username}/`

## Authentication

The only currently supported authentication option is using personal access
token.

### GitHub

See the GitHub documentation for personal access tokens:
[Link](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token).

The only required permission is the "repo" scope.

### GitHub

See the GitLab documentation for personal access tokens:
[Link](https://docs.gitlab.com/ee/user/profile/personal_access_tokens.html).

The required scopes are a bit weird. Actually, the following should suffice:

* * `read_user` to get user information (required to get the current authenticated
  user name for the `--owner` filter.
* A scope that allows reading private repositories. (`read_repository` is just
  for *cloning* private repos). This unfortunately does not exist.

So currently, you'll need to select the `read_api` scope.

## Filters

By default, `grm` will sync **nothing**. This is quite boring, so you have to
tell the command what repositories to include. They are all inclusive (i.e. act
as a logical OR), so you can easily chain many filters to clone a bunch of
repositories. It's quite simple:

* `--user <USER>` syncs all repositories of that remote user
* `--group <GROUP>` syncs all repositories of that remote group/organization
* `--owner` syncs all repositories of the user that is used for authentication.
  This is effectively a shortcut for `--user $YOUR_USER`
* `--access` syncs all repositories that the current user has access to

Easiest to see in an example:

```bash
$ grm repos sync remote --provider github --user torvals --owner --group zalando [...]
```

This would sync all of Torvald's repositories, all of my own repositories and
all (public) repositories in the "zalando" group.

## Strategies

There are generally three ways how you can use `grm` with forges:

### Ad-hoc cloning

This is the easiest, there are no local files involved. You just run the
command, `grm` clones the repos, that's it. If you run the command again, `grm`
will figure out the differences between local and remote repositories and
resolve them locally.

### Create a file

This is effectively `grm repos find local`, but using the forge instead of the
local file system. You will end up with a normal repository file that you can
commit to git. To update the list of repositories, just run the command again
and commit the new file.

### Define options in a file

This is a hybrid approach: You define filtering options in a file that you can
commit to source control. Effectively, you are persisting the options you gave
to `grm` on the command line with the ad-hoc approach. Similarly, `grm` will
figure out differences between local and remote and resolve them.

A file would look like this:

```toml
provider = "github"
token_command = "cat ~/.github_token"
root = "~/projects"

[filters]
owner = true
groups = [
  "zalando"
]
```

The options in the file map to the command line options of the `grm repos sync
remote` command.

You'd then run the `grm repos sync` command the same way as with a list of
repositories in a config:

```bash
$ grm repos sync --config example.config.toml
```

You can even use that file to generate a repository list that you can feed into
`grm repos sync`:

```bash
$ grm repos find config --config example.config.toml > repos.toml
$ grm repos sync config --config repos.toml
```

## Using with selfhosted GitLab

By default, `grm` uses the default GitLab API endpoint
([https://gitlab.com](https://gitlab.com)). You can override the
endpoint by specifying the `--api-url` parameter. Like this:

```bash
$ grm repos sync remote --provider gitlab --api-url https://gitlab.example.com [...]
```

## The cloning protocol

By default, `grm` will use HTTPS for public repositories and SSH otherwise. This
can be overridden with the `--force-ssh` switch.

## About the token command

To ensure maximum flexibility, `grm` has a single way to get the token it uses
to authenticate: Specify a command that returns the token via stdout. This easily
integrates with password managers like [`pass`](https://www.passwordstore.org/).

Of course, you are also free to specify something like `echo mytoken` as the
command, as long as you are ok with the security implications (like having the
token in cleartext in your shell history). It may be better to have the token
in a file instead and read it: `cat ~/.gitlab_token`.

Generally, use whatever you want. The command just has to return sucessfully and
return the token as the first line of stdout.

## Examples

Maybe you just want to locally clone all repos from your github user?

```bash
$ grm repos sync remote --provider github --owner --root ~/github_projects --token-command "pass show github_grm_access_token"
```

This will clone all repositories into `~/github_projects/{your_github_username}`.

If instead you want to clone **all** repositories you have access to (e.g. via
organizations or other users' private repos you have access to), just change the
filter a little bit:

```bash
$ grm repos sync remote --provider github --access --root ~/github_projects --token-command "pass show github_grm_access_token"
```

## Limitations

### GitHub

Unfortunately, GitHub does not have a nice API endpoint to get **private**
repositories for a certain user ([`/users/{user}/repos/`](https://docs.github.com/en/rest/repos/repos#list-repositories-for-a-user) only returns public
repositories).

Therefore, using `--user {user}` will only show public repositories for GitHub.
Note that this does not apply to `--access`: If you have access to another user's
private repository, it will be listed.

## Adding integrations

Adding a new integration involves writing some Rust code. Most of the logic is
generic, so you will not have to reinvent the wheel. Generally, you will need to
gather the following information:

* A list of repositories for a single user
* A list of repositories for a group (or any similar concept if applicable)
* A list of repositories for the user that the API token belongs to
* The username of the currently authenticated user

Authentication currently only works via a bearer token passed via the
`Authorization` HTTP header.

Each repo has to have the following properties:

* A name (which also acts as the identifier for diff between local and remote
  repositories)
* An SSH url to push to
* An HTTPS url to clone and fetch from
* A flag that marks the repository as private

If you plan to implement another forge, please first open an issue so we can
go through the required setup. I'm happy to help!
