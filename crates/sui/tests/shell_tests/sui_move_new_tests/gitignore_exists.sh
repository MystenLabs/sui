# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# check that sui move new correctly updates existing .gitignore
mkdir example
echo "existing_ignore" > example/.gitignore
sui move --client.config $CONFIG new example
cat example/.gitignore
