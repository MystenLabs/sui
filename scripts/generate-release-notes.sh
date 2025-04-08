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

echo -e "Sui Protocol Version in this release: XX\n"
for pr_number in $(git log --grep "\[x\]" --pretty=oneline --abbrev-commit origin/"${new_branch}"...origin/"${prev_branch}" -- crates dashboards doc docker external-crates kiosk nre sui-execution | grep -o '#[0-9]\+' | grep -o '[0-9]\+')
do
    pr_body=$(gh api -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28" /repos/MystenLabs/sui/pulls/"${pr_number}" --jq ".body")
    release_notes="${pr_body#*### Release notes}"
    echo -e "\nhttps://github.com/MystenLabs/sui/pull/${pr_number}: ${release_notes}"
done
