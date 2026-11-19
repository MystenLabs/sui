#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Run a local cluster (four validators, two fullnodes and two stress clients,
# after sui-operations' Antithesis compose file) deterministically under
# DERP (https://github.com/mlogan/derp): build the binaries with rooms for the
# rewriter, make the genesis, and run run.yaml under the supervisor.
#
#   scripts/derp/run.sh [--split REV] [derp run options...]
#
# --split REV makes a split cluster: the hosts in SPLIT_HOSTS (default
# "validator1 validator3 fullnode1") run sui-node built at REV, and the rest
# run this checkout's build. REV is checked out in a worktree that run.sh
# makes under ~/.cache/sui-derp-split and built into target/derp-split.
# REV's `sui` makes the genesis at REV's max protocol version. The run is
# refused unless that version is released (no later than the last version in
# sui-framework-snapshot's manifest) and has the same protocol-config snapshot
# here, because main changes its unreleased version in place. Stress always
# runs this checkout's build, so its Move packages must link against REV's
# framework.
#
# DERP_DIR is the DERP checkout (default ~/repos/derp), SCRATCH the run's
# directory (default scripts/derp/scratch: host directories, stdout.N and
# stderr.N per process), SUI_CARGO_FLAGS extra flags for the builds (such as
# --no-default-features, for the system allocator and a seeded heap).
# TIDEHUNTER=1 builds the nodes on tidehunter instead of RocksDB, into target
# directories of their own; in split mode REV must support the
# typed-store/tidehunter feature too.
#
# Concurrent runs from one checkout are not supported: they share bin/ and
# cluster/.
set -euo pipefail

# An exported CDPATH would make cd print the directory into these.
unset CDPATH
here=$(cd "$(dirname "$0")" && pwd)
sui_dir=$(cd "$here/../.." && pwd)
derp=$(cd "${DERP_DIR:-$HOME/repos/derp}" && pwd)
scratch=${SCRATCH:-$here/scratch}
venv=$here/.venv
validators="validator1 validator2 validator3 validator4"
hosts="$validators fullnode1 observer-fullnode1"
split_hosts=${SPLIT_HOSTS:-validator1 validator3 fullnode1}
usage="usage: run.sh [--split REV] [derp run options...]"

split_rev=
while [[ $# -gt 0 ]]; do
    case $1 in
        --split)
            [[ $# -ge 2 ]] || { echo "$usage" >&2; exit 1; }
            split_rev=$2
            shift 2
            ;;
        *) break ;;
    esac
done

if [[ -n $split_rev ]]; then
    split_validator=0
    for h in $split_hosts; do
        [[ " $hosts " == *" $h "* ]] || { echo "SPLIT_HOSTS: unknown host $h" >&2; exit 1; }
        if [[ " $validators " == *" $h "* ]]; then split_validator=1; fi
    done
    # With every validator on this checkout, the network upgrades past what REV supports.
    [[ $split_validator == 1 ]] || { echo "SPLIT_HOSTS must include a validator" >&2; exit 1; }

    split_sha=$(git -C "$sui_dir" rev-parse --verify "$split_rev^{commit}")
    genesis_version=$(git -C "$sui_dir" show "$split_sha:crates/sui-protocol-config/src/lib.rs" \
        | sed -n 's/^const MAX_PROTOCOL_VERSION: u64 = \([0-9][0-9]*\);$/\1/p')
    [[ $genesis_version =~ ^[0-9]+$ ]] || { echo "no MAX_PROTOCOL_VERSION found at $split_sha" >&2; exit 1; }
    released=$("${PYTHON:-/usr/bin/python3}" -c 'import json, sys; print(max(map(int, json.load(sys.stdin))))' \
        < "$sui_dir/crates/sui-framework-snapshot/manifest.json")
    if (( genesis_version > released )); then
        echo "protocol version $genesis_version of $split_sha is not released (latest: $released)" >&2
        exit 1
    fi
    snap=crates/sui-protocol-config/src/snapshots/sui_protocol_config__test__version_$genesis_version.snap
    git -C "$sui_dir" cat-file -e "$split_sha:$snap" 2>/dev/null \
        || { echo "$split_sha has no $snap" >&2; exit 1; }
    if ! cmp -s <(git -C "$sui_dir" show "$split_sha:$snap") "$sui_dir/$snap"; then
        echo "protocol version $genesis_version is configured differently at $split_sha and here ($snap)" >&2
        exit 1
    fi

    # Outside this checkout, so that its .cargo/config.toml does not apply to REV's build.
    split_src=$HOME/.cache/sui-derp-split/$(printf %s "$sui_dir" | shasum | cut -c1-12)/src
    if [[ ! -e $split_src/.git ]]; then
        mkdir -p "$(dirname "$split_src")"
        # -f replaces a registration left behind when the directory was deleted.
        git -C "$sui_dir" worktree add -f --detach "$split_src" "$split_sha"
    else
        # A build may have rewritten Cargo.lock; any other change would be built as REV.
        if [[ -n $(git -C "$split_src" status --porcelain --untracked-files=no -- . ':!Cargo.lock') ]]; then
            echo "$split_src has local changes; commit or discard them" >&2
            exit 1
        fi
        # Checking out only when needed keeps REV's binaries from relinking on every run.
        if ! git -C "$split_src" diff --quiet HEAD -- Cargo.lock; then
            git -C "$split_src" checkout HEAD -- Cargo.lock
        fi
        if [[ $(git -C "$split_src" rev-parse HEAD) != "$split_sha" ]]; then
            git -C "$split_src" checkout --detach "$split_sha"
        fi
    fi
fi

(cd "$derp" && CARGO_TARGET_DIR=$derp/target cargo build --release --workspace)
derp_bin=$derp/target/release/derp

flags=${SUI_CARGO_FLAGS:-}
suffix=
if [[ ${TIDEHUNTER:-} == 1 ]]; then
    export USE_TIDEHUNTER=1
    suffix=-tidehunter
    flags="$flags --features typed-store/tidehunter"
fi

# build SRC TARGET BIN...
build() {
    local src=$1 target=$2
    shift 2
    local bins=()
    for b in "$@"; do bins+=(--bin "$b"); done
    # shellcheck disable=SC2086
    (cd "$src" && CARGO_TARGET_DIR=$target "$derp_bin" cargo build --release $flags "${bins[@]}")
}

# The builds go to target directories of their own: `derp cargo` links
# through `derp cc`, and sharing target/ would rebuild it both ways.
target=$sui_dir/target/derp$suffix
rm -rf "$here/bin"
mkdir -p "$here/bin"
if [[ -n $split_rev ]]; then
    build "$sui_dir" "$target" sui-node stress
    split_target=$sui_dir/target/derp-split$suffix
    build "$split_src" "$split_target" sui-node sui
    genesis_sui=$split_target/release/sui
else
    build "$sui_dir" "$target" sui-node stress sui
    genesis_sui=$target/release/sui
fi
ln -s "$target/release/stress" "$here/bin/stress"
for h in $hosts; do
    mkdir -p "$here/bin/$h"
    ln -s "$target/release/sui-node" "$here/bin/$h/sui-node"
done
if [[ -n $split_rev ]]; then
    for h in $split_hosts; do
        ln -sf "$split_target/release/sui-node" "$here/bin/$h/sui-node"
    done
    echo "split cluster: $split_hosts run $split_sha; the rest run $(git -C "$sui_dir" describe --always --abbrev=40 --dirty --exclude="*")"
fi

# genesis.yaml fixes every key, so the genesis is the same on every run with the same `sui`.
if ! "$venv/bin/python" -c 'import yaml, cryptography' 2>/dev/null; then
    rm -rf "$venv"
    "${PYTHON:-/usr/bin/python3}" -m venv "$venv"
    "$venv/bin/pip" install --quiet pyyaml cryptography
fi
rm -rf "$here/cluster" "$here/cluster.tmp"
"$venv/bin/python" "$here/genesis.py" "$genesis_sui" "$here/cluster.tmp"
mv "$here/cluster.tmp" "$here/cluster"

exec "$derp_bin" run --manifest "$here/run.yaml" --capture --capture-stderr \
    --scratch "$scratch" "$@"
