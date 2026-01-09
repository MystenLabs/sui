// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Query transaction effects with valid fields and an error field (invalid pagination).
  # The response should have both data (for successful fields) AND errors (for failed field)
  transactionEffects(digest: "@{digest_1}") {
    # Valid fields - should return data
    status
    lamportVersion
    effectsDigest
    # Error field - first and last together is invalid
    events(first: 1, last: 1) {
      edges {
        node {
          contents {
            json
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # Test partial error at query root level
  # One query succeeds with all fields, another has partial error
  validEffects: transactionEffects(digest: "@{digest_1}") {
    status
    lamportVersion
  }

  partialErrorEffects: transactionEffects(digest: "@{digest_1}") {
    status
    # Error field - first and last together is invalid
    objectChanges(first: 1, last: 1) {
      edges {
        node {
          address
        }
      }
    }
  }
}
