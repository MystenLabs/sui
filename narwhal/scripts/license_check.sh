#!/bin/bash
# shellcheck disable=SC2044
# This script checks each file starts with a license comment
set -e
set -o pipefail

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
TOPLEVEL="${DIR}/../"

# Iterate over rust files not in the target directory
for i in $(find "$TOPLEVEL" -path "$TOPLEVEL/target" -prune -o -iname "*.rs" -print) 
do
  echo "Checking $i"
  CNT=$(head -n3 "$i" | grep -oEe '// (Copyright \(c\) 2022, Mysten Labs, Inc.|SPDX-License-Identifier: Apache-2.0)' | wc -l)
  if [ "$CNT" -lt 2 ]
  then
     echo "File $i has an incorrect license header"
     exit 1
  else
     echo "File $i is OK"
  fi
done

exit 0
