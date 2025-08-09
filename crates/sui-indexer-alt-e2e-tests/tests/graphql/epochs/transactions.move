// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# advance-epoch

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 43 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# programmable --sender A --inputs 2
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# create-checkpoint

//# programmable --sender A --inputs 3
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# advance-epoch

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs 2
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs 3
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# run-graphql
{ # Fetch an epoch and its transactions, should have 2 transactions.
  # One in Checkpoint 0, one in Checkpoint 1.
  epoch(epochId: 0) {
    totalTransactions
    transactions(first: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql
{ # Fetch an epoch and fetch transactions after checkpoint 0.
  # This should have 1 transaction in Checkpoint 1.
  epoch(epochId: 0) {
    totalTransactions
    transactions(first: 1, filter: { afterCheckpoint: 0 }) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql
{ # Fetch an epoch and its transactions
  epoch(epochId: 1) {
    totalTransactions
    transactions(first: 10) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql --cursors 4 8
{ # Fetch an epoch and its transactions, with cursors applied to transactions
  epoch(epochId: 1) {
    totalTransactions
    transactions(first: 10, after: "@{cursor_0}", before: "@{cursor_1}") {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql
{ # Fetch an epoch and its transactions, at a checkpoint
  epoch(epochId: 1) {
    totalTransactions
    transactions( first: 3, filter: { atCheckpoint: 5 }) {
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql
{ # Fetch an epoch and its transactions, after a checkpoint
  epoch(epochId: 1) {
    totalTransactions
    transactions( first: 3, filter: { afterCheckpoint: 2 }) {
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql --cursors 5
{ # Fetch an epoch and its transactions, after a checkpoint with cursors applied
  epoch(epochId: 1) {
    totalTransactions
    transactions( first: 3, filter: { afterCheckpoint: 2 }, after: "@{cursor_0}") {
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql
{ # Fetch an epoch and its transactions, before a checkpoint
  epoch(epochId: 1) {
    totalTransactions
    transactions( first: 3, filter: { beforeCheckpoint: 3 }) {
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql
{ # Fetch nonexistent epoch
  epoch(epochId: 5) {
    totalTransactions
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