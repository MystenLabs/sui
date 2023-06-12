#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Generate Release Notes

if [ $# -lt 2 ];
then
    echo "./generate-release-notes.sh [previous branch] [new branch]"
    exit
else
    prev_branch=$1
    new_branch=$2
fi

for commit in $(git log --grep "\[x\]" --pretty=oneline --abbrev-commit origin/"${new_branch}"...origin/"${prev_branch}")
do
    regex='.*\(#([0-9]+)\).*'
    if [[ $commit =~ $regex ]]; then
        pr_number="${BASH_REMATCH[1]}"
        pr_body=$(gh api -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28" /repos/MystenLabs/sui/pulls/"${pr_number}" --jq ".body")
        release_notes="${pr_body#*### Release notes}"
        printf 'PR: \t%s\n' "$pr_number"
        echo "================"
        echo "${release_notes}"
    fi
done


