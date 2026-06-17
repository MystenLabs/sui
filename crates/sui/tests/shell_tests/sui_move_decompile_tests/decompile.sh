# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that sui move build output can be decompiled through the Sui CLI

sui move --client.config $CONFIG new example > /dev/null 2>&1

cat > example/sources/example.move <<EOF
module example::example;

public fun foo(): u64 {
    42
}
EOF

cd example
sui move --client.config $CONFIG build > /dev/null 2>&1
sui move --client.config $CONFIG decompile \
  --input build/example/bytecode_modules/example.mv \
  --output decompiled > /dev/null 2>&1

set -- decompiled/*/example.move
test -f "$1"
grep -q "module example::example" "$1"
grep -q "foo" "$1"

cat "$1"
