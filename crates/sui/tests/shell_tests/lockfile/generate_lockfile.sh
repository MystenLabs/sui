# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

echo "Run a build and verify that the correct lockfile is produced"
sui move --client.config $CONFIG build -p a

echo "Lockfile"
echo "==="
# On windows, the paths in the lockfile contain `\` so when they are written to TOML, toml_edit uses
# raw strings (' delimited) instead of regular strings (" delimited); thus we use `sed` instead of `cat`
#
# Note: we don't need to change the `\` because the shell tests do that
# automatically on the entire output
sed "s/'/\"/g" a/Move.lock
cp a/Move.lock a/Move.lock.tmp
echo "==="

echo "Rebuild and ensure that the lockfile hasn't changed"
sui move --client.config $CONFIG build -p a

echo "Diff"
echo "==="
diff a/Move.lock a/Move.lock.tmp
echo "==="
