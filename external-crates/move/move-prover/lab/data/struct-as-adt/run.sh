#!/bin/sh
# Copyright (c) The Diem Core Contributors
# Copyright (c) The Move Contributors
# SPDX-License-Identifier: Apache-2.0

echo "Lab cannot be run at head"
exit 1

DIEM="$(git rev-parse --show-toplevel)"
FRAMEWORK="$DIEM/diem-move/diem-framework/core/sources"
STDLIB="$DIEM/language/move-stdlib/sources"

for config in *.toml ; do
  # Benchmark per function
  cargo run -q --release -p prover-lab -- \
    bench -f -c $config -d $STDLIB -d $FRAMEWORK $FRAMEWORK/*.move
  # Benchmark per module
  cargo run -q --release -p prover-lab -- \
    bench -c $config -d $STDLIB -d $FRAMEWORK $FRAMEWORK/*.move
done
