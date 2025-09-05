// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --simulator

//# advance-epoch --create-random-state

//# set-random-state --randomness-round 0 --random-bytes SGVsbG8gU3VpIFJhbmRvbW5lc3M= --randomness-initial-version 2

//# create-checkpoint

//# run-graphql
{
  # Test the RandomnessStateUpdate transaction created by set-random-state
  randomnessStateUpdateTransaction: transaction(digest: "@{digest_2}") {
    digest
    kind {
      __typename
      ... on RandomnessStateUpdateTransaction {
        epoch
        randomnessRound
        randomBytes
        randomnessObjInitialSharedVersion
      }
    }
  }
} 
