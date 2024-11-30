#!/usr/bin/env bash

if ! git remote | grep -q ^aur$; then
    git remote add aur ssh://aur@aur.archlinux.org/grm-git.git
fi

git subtree push --prefix pkg/arch/ aur master

git remote rm aur
