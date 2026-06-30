// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --accounts A B --simulator

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
      edges { cursor node { digest, sender { address } } }
    }
  }
}

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 43 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs 2
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs 3
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Fetch a checkpoint's transactions filtered by sentAddress, should have sequence numbers 1,2,3,4,5
  checkpoint(sequenceNumber: 1) {
    sequenceNumber
    digest
    transactions(first: 5, filter: { sentAddress: "@{A}" }) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest, sender { address } } }
    }
    allTransactions: transactions(first: 6) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest, sender { address } } }
    }
  }
}

//# run-graphql
{ # Fetch non-existent checkpoint
  checkpoint(sequenceNumber: 6) {
    sequenceNumber
    digest
    transactions(first: 1) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest, sender { address } } }
    }
  }
}
