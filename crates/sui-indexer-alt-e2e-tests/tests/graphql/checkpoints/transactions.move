// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# run-graphql
{ # Fetch a checkpoint
  checkpoint(sequenceNumber: 0) {
    sequenceNumber
    digest
    transactions(first: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest } }
    }
  }
}

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Fetch a checkpoint
  checkpoint(sequenceNumber: 1) {
    sequenceNumber
    digest
    transactions(first: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest } }
    }
  }
}

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Fetch a checkpoint
  checkpoint(sequenceNumber: 2) {
    sequenceNumber
    digest
    transactions(first: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest } }
    }
  }
}

//# run-graphql
{ # Fetch a non-existent checkpoint
  checkpoint(sequenceNumber: 3) {
    sequenceNumber
    digest
    transactions(first: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest } }
    }
  }
}