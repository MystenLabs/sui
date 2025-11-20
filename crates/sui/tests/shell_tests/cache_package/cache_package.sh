# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

export GIT_CONFIG_GLOBAL=""

git init -q -b main a
git -C a add .
git -C a -c user.email=test@test.com -c user.name=test commit -q -m "initial revision"

HASH=$(git -C a log --pretty=format:%H)

# Run the `cache-package` command
sui move cache-package testnet 4c78adac "{ git = \"a\", rev = \"${HASH}\", subdir = \".\" }"
