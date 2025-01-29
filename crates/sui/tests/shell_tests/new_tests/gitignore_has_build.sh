# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# sui move new example when `example/.gitignore` already contains build/*; it should be unchanged
mkdir example
echo "ignore1" >> example/.gitignore
echo "build/*" >> example/.gitignore
echo "ignore2" >> example/.gitignore
sui move new example
cat example/.gitignore
echo
echo ==== files in example/ ====
ls -A example
