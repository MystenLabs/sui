#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# check if API_USER and API_KEY env vars are set
if [ -z "$API_USER" ] || [ -z "$API_KEY" ]; then
  echo "Error: API_USER and API_KEY environment variables must be set"
  exit 1
fi

NETWORK=$1

REPO_ROOT=$(git rev-parse --show-toplevel)
cd $REPO_ROOT

if [ "$NETWORK" != "devnet" ] && [ "$NETWORK" != "testnet" ] && [ "$NETWORK" != "mainnet" ]; then
  echo "Invalid network: $NETWORK"
  echo "Usage: check-protocol-compatibility.sh <devnet|testnet|mainnet> <git|prometheus>"
  exit 1
fi

case "$NETWORK" in
  devnet)
    URL="https://$API_USER:$API_KEY@gateway.mimir.sui.io/prometheus/api/v1/query"
    ;;
  testnet)
    URL="http://$API_USER:$API_KEY@metrics-gw.testnet.sui.io/prometheus/api/v1/query"
    ;;
  mainnet)
    URL="https://$API_USER:$API_KEY@metrics-gw.mainnet.sui.io/prometheus/api/v1/query"
    ;;
esac

VERSIONS=$(curl -s -G -k "$URL" --data-urlencode "query=uptime{network=\"$NETWORK\"}" | jq -r '.data.result[].metric.version' | sort | uniq -c | sort -rn)
TOP_VERSION=$(echo "$VERSIONS" | head -n 1 | awk '{print $2}')

echo "Found following versions on $NETWORK:"
echo "$VERSIONS"
echo ""
echo "Using most frequent version $TOP_VERSION for compatibility check"

# TOP_VERSION looks like "1.0.0-ae1212baf8", split out the commit hash
ORIGIN_COMMIT=$(echo "$TOP_VERSION" | cut -d- -f2)

echo "Checking protocol compatibility with $NETWORK ($ORIGIN_COMMIT)"

git fetch -q || exit 1

# put code to check if git client is clean into function
function check_git_clean {
  message=$1
  # if any files are edited or staged, exit with error
  if ! git diff --quiet --exit-code || ! git diff --cached --quiet --exit-code; then
    echo "Error: $message"
    exit 1
  fi
}

check_git_clean "Please commit or stash your changes before running this script"

# check out all files in crates/sui-protocol-config/src/snapshots at origin commit
echo "Checking out $NETWORK snapshot files"
git checkout $ORIGIN_COMMIT -- crates/sui-protocol-config/src/snapshots || exit 1

echo "Checking for changes to snapshot files"
check_git_clean "Detected changes to snapshot files since $ORIGIN_COMMIT - not safe to release"

echo "Running snapshot tests..."
cargo test --package sui-protocol-config snapshot_tests || exit 1

exit 0
