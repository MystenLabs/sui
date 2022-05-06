#!/bin/bash
# Copyright (c) 2022, Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# shellcheck disable=SC2044,SC2086,SC2016
# This script checks each file starts with a license comment
# Using -f argument the script will prepend (fix) the license info instead of failing
set -e
set -o pipefail

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
TOPLEVEL="${DIR}/../"
FIX=""

while getopts "f" arg; do
  case "${arg}" in
    f) FIX=true;;
  esac
done

# Iterate over files in the repo that satisfy the following rules
# 1. File extension is one of .(move | rs | tsx | ts | js)
# 2. File directory is not '$TOPLEVEL/target' or "**/build" or "**/node_modules"
for i in $(find $TOPLEVEL  -type d \( -path '$TOPLEVEL/target' -o -name 'node_modules' -o -name 'build' -o -name 'dist' \) -prune -o \( -iname '*.rs' -o -iname '*.move' -o -iname '*.tsx' -o -iname '*.ts' -o -iname '*.js' \) -print)
do
  CNT=$(head -n3 "$i" | grep -oEe '// (Copyright \(c\) 2022, Mysten Labs, Inc.|SPDX-License-Identifier: Apache-2.0)' | wc -l) || true
  if [ "$CNT" -lt 2 ]
  then
    echo -n "File $i has an incorrect license header"
    if [ $FIX ]; then
      echo -e "// Copyright (c) 2022, Mysten Labs, Inc.\n// SPDX-License-Identifier: Apache-2.0\n\n$(cat $i)" > $i
      echo '...[FIXED ✔]'
    else
      exit 1
    fi
  fi
done
