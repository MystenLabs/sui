#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Build & push the k6 gRPC load-test image.
#
# Unlike the sibling docker/*/build.sh scripts, this stages the canonical k6
# script, committed request lists, and merged proto tree before building from
# grafana/k6. The committed stage is the deployable CI/meta-svc input and this
# script regenerates it from the canonical sources.
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
# Context lives OUTSIDE docker/ on purpose: the repo-root .dockerignore excludes
# `docker/`, and meta-svc's fsutil does NOT honor `!` negations to rescue it
# (proven empirically -- negations work on local BuildKit but not meta-svc). A
# path under grpc-list-testing/ matches no exclusion, so it always transfers.
CTX="${CTX:-$GRPC_TESTING_DIR/loadtest-context}"

[[ -d "$GRPC_TESTING_DIR" ]] || { echo "ERROR: GRPC_TESTING_DIR not found: $GRPC_TESTING_DIR" >&2; exit 1; }

# --- stage the build context (inputs only; Dockerfile is passed to meta-svc via
#     --dockerfile and to local builds via -f, never copied into the context) ---
rm -rf "$CTX"; mkdir -p "$CTX/data" "$CTX/proto"

# 1. load script
cp "$GRPC_TESTING_DIR/load.k6.js" "$CTX/load.k6.js"

# 2. generated request lists (bake EVERY net present; deploy picks via REQ_FILE).
#    NET still gates that AT LEAST that net's list exists (fail-fast).
REQ="$GRPC_TESTING_DIR/load.${NET}.jsonl"
[[ -f "$REQ" ]] || { echo "ERROR: $REQ not found; run: python gen_load.py $NET ..." >&2; exit 1; }
for f in "$GRPC_TESTING_DIR"/load.*.jsonl; do
  [[ -e "$f" ]] || continue
  cp "$f" "$CTX/data/$(basename "$f")"   # -> context/data/load.<net>.jsonl
  echo "  staged $(basename "$f")"
done

# 3. Merge the stable-v2 List entry and imports from vendored/proto with the
#    root proto tree. The latter retains the independent v2alpha ProofService;
#    the disjoint paths share one /proto import root.
ROOT="${SUI_RPC_PROTO:-}"
if [[ -z "$ROOT" ]]; then
  for d in "$HOME"/.cargo/git/checkouts/sui-rust-sdk-*/*/crates/sui-rpc; do
    if [[ -f "$d/vendored/proto/sui/rpc/v2/ledger_service.proto" ]]; then ROOT="$d"; break; fi
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
echo "  data   : $(cd "$CTX/data" && ls load.*.jsonl | tr '\n' ' ')($(cat "$CTX"/data/load.*.jsonl | wc -l | tr -d ' ') total requests)"
echo "  proto  : $(cd "$CTX/proto" && find . -name '*.proto' | wc -l | tr -d ' ') files (root: $ROOT)"
echo "  image  : $IMAGE"
echo

# --- build + push (ENGINE=container [default] | docker) ---------------------
# Context = REPO ROOT (matches meta-svc), Dockerfile from docker/ via -f, COPY
# paths repo-root-relative. Local `container`/`docker` only reads what the
# Dockerfile COPYs, so the big repo context is cheap here.
DOCKERFILE="$DIR/Dockerfile"
ENGINE="${ENGINE:-container}"
case "$ENGINE" in
  container)
    container build --platform linux/amd64 -f "$DOCKERFILE" -t "$IMAGE" "$REPO_ROOT"
    if [[ "${PUSH:-1}" == "1" ]]; then
      container image push "$IMAGE"
    fi
    ;;
  docker)
    docker build --platform linux/amd64 -f "$DOCKERFILE" -t "$IMAGE" "$REPO_ROOT"
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
