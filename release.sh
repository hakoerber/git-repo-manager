#!/usr/bin/env bash

set -o nounset
set -o errexit
set -o pipefail

usage() {
    printf '%s\n' "usage: $0 (master|minor|patch)" >&2
}

if (( $# != 1 )) ; then
    usage
    exit 1
fi

current_version="$(grep '^version \?=' Cargo.toml | head -1 | cut -d '=' -f 2 | tr -d " '"'"')"

major="$(printf '%s' "${current_version}" | grep -oP '^\d+')"
minor="$(printf '%s' "${current_version}" | grep -oP '\.\d+\.' | tr -d '.')"
patch="$(printf '%s' "${current_version}" | grep -oP '\d+$' | tr -d '.')"

case "$1" in
    major)
        (( major++ )) || true
        minor=0
        patch=0
        ;;
    minor)
        (( minor++ )) || true
        patch=0
        ;;
    patch)
        (( patch++ )) || true
        ;;
    *)
        usage
        exit 1
        ;;
esac

new_version="${major}.${minor}.${patch}"

if ! [[ "${new_version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ; then
    printf '%s\n' 'Version has to a complete semver' >&2
    exit 1
fi

current_branch="$(git rev-parse --abbrev-ref HEAD)"
if [[ "${current_branch}" != "develop" ]] ; then
    printf '%s\n' 'You need to be on develop' >&2
    exit 1
fi

gitstatus="$(git status --porcelain)"
if [[ -n "${gitstatus}" ]] ; then
    printf '%s\n' 'There are uncommitted changes' >&2
    exit 1
fi

if git tag --list "v${new_version}" | grep -q . ; then
    printf 'Tag %s already exists\n' "v${new_version}" >&2
    exit 1
fi

for remote in $(git remote) ; do
    if git ls-remote --tags "${remote}" | grep -q "refs/tags/v${new_version}$" ; then
        printf 'Tag %s already exists on %s' "v${new_version}" "${remote}" >&2
        exit 1
    fi
done

git fetch --all

for remote in $(git remote) ; do
    for branch in master develop ; do
        if ! git diff --quiet "${remote}/${branch}..${branch}" ; then
            printf 'Remote branch %s/%s not up to date, synchronize first!\n' "${remote}" "${branch}" >&2
            exit 1
        fi
    done
done

if ! git merge-base --is-ancestor master develop ; then
    printf '%s\n' 'Develop is not a straight descendant of master, rebase!' >&2
    exit 1
fi

changes="$(git log --oneline master..develop | wc -l)"
if (( changes == 0 )) ; then
    printf '%s\n' 'No changes between master and develop?' >&2
    exit 1
fi

just update-dependencies

just check

sed -i "0,/^version/{s/^version.*$/version = \"${new_version}\"/}" Cargo.toml

cargo update --package git-repo-manager --precise "${new_version}"

diff="$(git diff --numstat)"
if (( $(printf '%s\n' "${diff}" | wc -l || true) != 2 )) ; then
     printf '%s\n' 'Weird changes detected, bailing' >&2
     exit 1
fi

if ! printf '%s\n' "${diff}" | grep -Pq '^1\s+1\s+Cargo.lock$' ; then
     printf '%s\n' 'Weird changes detected, bailing' >&2
     exit 1
fi

if ! printf '%s\n' "${diff}" | grep -Pq '^1\s+1\s+Cargo.toml$' ; then
     printf '%s\n' 'Weird changes detected, bailing' >&2
     exit 1
fi

git add Cargo.lock Cargo.toml

git commit -m "Release v${new_version}"

git switch master 2>/dev/null || { [[ -d "../master" ]] && cd "../master" ; } || { printf '%s\n' 'Could not change to master' >&2 ; exit 1 ; }

current_branch="$(git rev-parse --abbrev-ref HEAD)"
if [[ "${current_branch}" != "master" ]] ; then
    printf '%s\n' 'Looks like branch switching to master did not work' >&2
    exit 1
fi

git merge --no-ff --no-edit develop
git tag "v${new_version}"

for remote in $(git remote) ; do
    while ! git push "${remote}" "v${new_version}" master ; do
        :
    done
done

git switch develop 2>/dev/null || { [[ -d "../develop" ]] && cd "../develop" ; } || { printf '%s\n' 'Could not change to develop' >&2 ; exit 1 ; }

current_branch="$(git rev-parse --abbrev-ref HEAD)"
if [[ "${current_branch}" != "develop" ]] ; then
    printf '%s\n' 'Looks like branch switching to develop did not work' >&2
    exit 1
fi

git merge --ff-only master

for remote in $(git remote) ; do
    while ! git push "${remote}" develop ; do
        :
    done
done

cargo publish

printf 'Published %s successfully\n' "${new_version}"
exit 0
