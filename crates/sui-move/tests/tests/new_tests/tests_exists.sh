# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# sui-move new example when example/tests exists should not generate any new example source but should otherwise
# operate normally

mkdir -p example/tests
echo "existing_ignore" >> example/.gitignore

sui-move new example
echo ==== project files ====
ls example
echo ==== sources ====
ls example/sources
echo ==== tests ====
ls example/tests
echo ==== .gitignore ====
cat example/.gitignore
