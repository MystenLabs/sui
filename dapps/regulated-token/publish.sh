#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

PUBLISH=$(sui client publish --gas-budget 100000000 --skip-dependency-verification --with-unpublished-dependencies --json);

# then we need to filter object changes and find the one where the type includes "TreasuryCap"
# and then we need to get the packageId from that object change
package=$(jq '.objectChanges[] | select(.type != null) | select(.type == "published")' <<< "$PUBLISH" | jq -r '.packageId');
policy=$(jq '.objectChanges[] | select(.objectType != null) | select(.objectType | contains("::token::TokenPolicy<"))' <<< "$PUBLISH" | jq -r '.objectId');
policy_cap=$(jq '.objectChanges[] | select(.objectType != null) | select(.objectType | contains("::token::TokenPolicyCap"))' <<< "$PUBLISH" | jq -r '.objectId');
treasury_cap=$(jq '.objectChanges[] | select(.objectType != null) | select(.objectType | contains("TreasuryCap"))' <<< "$PUBLISH" | jq -r '.objectId');

echo;
echo "Environment variables for the next step:"
echo "PKG=$package" > .env
echo "POLICY=$policy" >> .env
echo "POLICY_CAP=$policy_cap" >> .env
echo "TREASURY_CAP=$treasury_cap" >> .env
cat .env
echo;
echo "To apply them to current process, run: 'source .env'"
