#!/bin/bash
# Copyright (c) 2022, Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# shellcheck disable=SC2044,SC2086,SC2016
# This script checks each file starts with a license comment
set -e
set -o pipefail

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
TOPLEVEL="${DIR}/../"

# Iterate over files in the repo that satisfy the following rules
# 1. File extension is one of .(move | rs | tsx | ts | js)
# 2. File directory is not '$TOPLEVEL/target' or "**/build" or "**/node_modules"
for i in $(find $TOPLEVEL  -type d \( -path '$TOPLEVEL/target' -o -name 'node_modules' -o -name 'build' -o -name 'dist' \) -prune -o \( -iname '*.rs' -o -iname '*.move' -o -iname '*.tsx' -o -iname '*.ts' -o -iname '*.js' \) -print)
do
  CNT=$(head -n3 "$i" | grep -oEe '// (Copyright \(c\) 2022, Mysten Labs, Inc.|SPDX-License-Identifier: Apache-2.0)' | wc -l) || true
  if [ "$CNT" -lt 2 ]
  then
    echo "File $i has an incorrect license header"
    exit 1
  fi
done
