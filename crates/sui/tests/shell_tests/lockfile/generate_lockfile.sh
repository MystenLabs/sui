# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

echo "Run a build and verify that the correct lockfile is produced"
sui move --client.config $CONFIG build -p a

echo "Lockfile"
echo "==="
cat a/Move.lock
cp a/Move.lock a/Move.lock.tmp
echo "==="

echo "Rebuild and ensure that the lockfile hasn't changed"
sui move --client.config $CONFIG build -p a

echo "Diff"
echo "==="
diff a/Move.lock a/Move.lock.tmp
echo "==="
