#!/usr/bin/env bash

set -o nounset
set -o errexit

# shellcheck disable=SC1091
source ./venv/bin/activate

pip --disable-pip-version-check install -r ./requirements.txt

pip3 list --outdated --format=freeze | grep -v '^\-e' | cut -d = -f 1 | while read -r package ; do
    [[ package == "pip" ]] && continue
    [[ package == "setuptools" ]] && continue
    pip install --upgrade "${package}"
    version="$(pip show "${package}" | grep '^Version' | cut -d ' ' -f 2)"
    message="e2e_tests/pip: Update ${package} to ${version}"
    pip freeze | grep -v '^pkg_resources' > requirements.txt
    git add ./requirements.txt
    git commit --message "${message}"
done
