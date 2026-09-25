#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Run a local cluster (four validators, two fullnodes and the stress client,
# after sui-operations' Antithesis compose file) deterministically under
# DERP (https://github.com/mlogan/derp): build sui-node, stress and sui from
# this checkout with rooms for the rewriter, make the genesis once, and run
# run.yaml under the supervisor.
#
#   scripts/derp/run.sh [--regenesis] [derp run options...]
#
# DERP_DIR is the DERP checkout (default ~/repos/derp), SCRATCH the run's
# directory (default scripts/derp/scratch: host directories, stdout.N and
# stderr.N per process), SUI_CARGO_FLAGS extra flags for the build (such as
# --no-default-features, for the system allocator and a seeded heap).
# TIDEHUNTER=1 builds the nodes on tidehunter instead of RocksDB, into a
# target directory of their own: the two stores' databases are not
# compatible.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
sui_dir=$(cd "$here/../.." && pwd)
derp=${DERP_DIR:-$HOME/repos/derp}
scratch=${SCRATCH:-$here/scratch}
venv=$here/.venv

regenesis=0
if [[ ${1:-} == --regenesis ]]; then
    regenesis=1
    shift
fi

(cd "$derp" && cargo build --release --workspace)
derp_bin=$derp/target/release/derp

target=$sui_dir/target
flags=${SUI_CARGO_FLAGS:-}
if [[ ${TIDEHUNTER:-} == 1 ]]; then
    export USE_TIDEHUNTER=1
    target=$sui_dir/target/tidehunter
    flags="$flags --features typed-store/tidehunter"
fi
# shellcheck disable=SC2086
(cd "$sui_dir" && CARGO_TARGET_DIR=$target "$derp_bin" cargo build --release $flags \
    --bin sui-node --bin stress --bin sui)
mkdir -p "$here/bin"
for prog in sui-node stress sui; do
    ln -sf "$target/release/$prog" "$here/bin/$prog"
done

# The genesis is made natively and kept: its keys are random, and a run
# is only repeatable with the same ones. Make it again after rebuilding
# sui at another commit.
if [[ $regenesis == 1 || ! -f $here/cluster/genesis.blob ]]; then
    if ! "$venv/bin/python" -c 'import yaml, cryptography' 2>/dev/null; then
        "${PYTHON:-python3}" -m venv "$venv"
        "$venv/bin/pip" install --quiet pyyaml cryptography
    fi
    rm -rf "$here/cluster"
    "$venv/bin/python" "$here/genesis.py" "$here/bin/sui" "$here/cluster"
fi

exec "$derp_bin" run --manifest "$here/run.yaml" --capture --capture-stderr \
    --scratch "$scratch" "$@"
