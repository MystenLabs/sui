#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Build & push the k6 gRPC load-test image.
#
# Unlike the sibling docker/*/build.sh (which cargo-build a Rust binary with the
# repo root as context), this stages a small context (k6 script + merged proto
# tree + the generated request list) and builds FROM grafana/k6. The request list
# and protos are generated/vendored (not committed), so they are staged here at
# build time -- the image is the frozen, deployable artifact (reproducible from
# grpc-list-testing/load_manifest.<net>.json: {seed, pool, mixes}).
#
# Inputs (env, with defaults):
#   NET               mainnet | testnet   (default testnet) -> bakes load.<NET>.jsonl
#   GRPC_TESTING_DIR  sui worktree's grpc-list-testing dir
#                     (default: <repo-root>/grpc-list-testing)
#   SUI_RPC_PROTO     override proto root (…/crates/sui-rpc); else auto-discovered
#   REGISTRY          AR repo (default us-central1-docker.pkg.dev/cryptic-bolt-398315/grpc-loadtest)
#   TAG               image tag (default: <NET>-<sha256(context)[:12]>)
#   PUSH              1 to docker push after build (default 1); 0 to build only
#
# Usage:
#   NET=testnet ./build.sh
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(git -C "$DIR" rev-parse --show-toplevel)"
NET="${NET:-testnet}"
GRPC_TESTING_DIR="${GRPC_TESTING_DIR:-$REPO_ROOT/grpc-list-testing}"
REGISTRY="${REGISTRY:-us-central1-docker.pkg.dev/cryptic-bolt-398315/grpc-loadtest}"
CTX="$DIR/context"

[[ -d "$GRPC_TESTING_DIR" ]] || { echo "ERROR: GRPC_TESTING_DIR not found: $GRPC_TESTING_DIR" >&2; exit 1; }

# --- stage the build context ------------------------------------------------
rm -rf "$CTX"; mkdir -p "$CTX/data" "$CTX/proto"
cp "$DIR/Dockerfile" "$CTX/Dockerfile"

# 1. load script
cp "$GRPC_TESTING_DIR/load.k6.js" "$CTX/load.k6.js"

# 2. generated request list (baked; swap NET to change corpus)
REQ="$GRPC_TESTING_DIR/load.${NET}.jsonl"
[[ -f "$REQ" ]] || { echo "ERROR: $REQ not found; run: python gen_load.py $NET ..." >&2; exit 1; }
cp "$REQ" "$CTX/data/load.jsonl"

# 3. merge the two proto roots into one tree (v2alpha under proto/, v2+google
#    under vendored/proto/; disjoint paths -> single /proto root serves both,
#    cf. grpc-list-testing/correctness/gen_stubs.sh's -I x2).
ROOT="${SUI_RPC_PROTO:-}"
if [[ -z "$ROOT" ]]; then
  for d in "$HOME"/.cargo/git/checkouts/sui-rust-sdk-*/*/crates/sui-rpc; do
    if [[ -f "$d/proto/sui/rpc/v2alpha/ledger_service.proto" ]]; then ROOT="$d"; break; fi
  done
fi
[[ -n "$ROOT" && -d "$ROOT" ]] || { echo "ERROR: sui-rpc proto root not found; set SUI_RPC_PROTO" >&2; exit 1; }
cp -R "$ROOT/proto/." "$CTX/proto/"
cp -R "$ROOT/vendored/proto/." "$CTX/proto/"

# --- tag: NET + content hash of the staged context -> fresh tag on any change
if [[ -z "${TAG:-}" ]]; then
  HASH="$(cd "$CTX" && find . -type f -exec shasum -a 256 {} \; | LC_ALL=C sort | shasum -a 256 | cut -c1-12)"
  TAG="${NET}-${HASH}"
fi
IMAGE="$REGISTRY/k6:$TAG"

echo
echo "Building grpc-loadtest k6 image"
echo "  net    : $NET"
echo "  context: $CTX"
echo "  script : load.k6.js"
echo "  data   : $(wc -l < "$CTX/data/load.jsonl" | tr -d ' ') requests (from load.${NET}.jsonl)"
echo "  proto  : $(cd "$CTX/proto" && find . -name '*.proto' | wc -l | tr -d ' ') files (root: $ROOT)"
echo "  image  : $IMAGE"
echo

# --- build + push (ENGINE=container [default] | docker) ---------------------
ENGINE="${ENGINE:-container}"
case "$ENGINE" in
  container)
    container build --platform linux/amd64 -f "$CTX/Dockerfile" -t "$IMAGE" "$CTX"
    if [[ "${PUSH:-1}" == "1" ]]; then
      container image push "$IMAGE"
    fi
    ;;
  docker)
    docker build --platform linux/amd64 -f "$CTX/Dockerfile" -t "$IMAGE" "$CTX"
    if [[ "${PUSH:-1}" == "1" ]]; then
      docker push "$IMAGE"
    fi
    ;;
  *)
    echo "ERROR: unknown ENGINE=$ENGINE (want: container | docker)" >&2; exit 1 ;;
esac

if [[ "${PUSH:-1}" == "1" ]]; then
  echo
  echo "Pushed ($ENGINE). Set this in the Pulumi stack config:"
  echo "  grpc-loadtest:deploy_config.image: $IMAGE"
fi
