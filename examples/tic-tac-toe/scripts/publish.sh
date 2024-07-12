#! /usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

set -e

# Change to the script's directory.
cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null

# Check dependencies are available.
SUI=${SUI:-sui}
for i in jq $SUI; do
  if ! command -V ${i} &>/dev/null; then
    echo "${i} is not installed"
    exit 1
  fi
done

# Make sure an environment was provided, and switch to it
if [ -z "$1" ]; then
    echo "Error: No environment provided."
    exit 1
fi
ENV=$1; shift
$SUI client switch --env $ENV

PUBLISH=$($SUI client publish ../move --json $@)

STATUS=$(
    echo $PUBLISH |
        jq -r '.effects.status.status'
)

if [[ $STATUS != "success" ]]; then
    echo "Error: Move contract publishing failed. Status:"
    echo $PUBLISH | jq '.effects.status'
    exit 1
fi

PACKAGE_ID=$(
    echo $PUBLISH |
        jq -r '.objectChanges[] | select(.type == "published") | .packageId'
)

UPGRADE_CAP=$(
    echo $PUBLISH |
        jq -r '.objectChanges[]
            | select(.type == "created")
            | select(.objectType | contains("0x2::package::UpgradeCap"))
            | .objectId'
)

CONFIG="$(readlink -f ../ui/src)/env.$ENV.ts"
cat > $CONFIG <<EOF
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export default {
	packageId: '$PACKAGE_ID',
	upgradeCap: '$UPGRADE_CAP',
};
EOF

ENV="$(readlink -f ../cli)/$ENV.env"
cat > $ENV <<-EOF
PKG=$PACKAGE_ID
CAP=$UPGRADE_CAP
EOF

echo "Contract Deployment finished!"
echo "Details written to $CONFIG and $ENV."
