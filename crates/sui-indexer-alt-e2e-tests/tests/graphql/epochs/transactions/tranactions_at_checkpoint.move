// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# advance-epoch

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 43 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))
//# create-checkpoint

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# programmable --sender A --inputs 2
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# run-graphql
{ # Fetch an epoch and its transactions, at a checkpoint
  epoch0AllTransactions: epoch(epochId: 0) {
        totalTransactions
        transactions( first: 3) {
            edges { cursor node { ...TransactionFragment } } 
        }
  }
  epoch0AtCheckpoint0: epoch(epochId: 0) {
    totalTransactions
    transactions( first: 3, filter: { atCheckpoint: 0 }) {
      edges { cursor node { ...TransactionFragment } } 
    }
  }
  # Test filtering for a checkpoint that does not exist in an epoch
  epochWithNonexistentCheckpoint: epoch(epochId: 0) {
    totalTransactions
    transactions( first: 3, filter: { atCheckpoint: 10 }) {
      edges { cursor node { ...TransactionFragment } } 
    }
  }
  epoch1AtCheckpoint2: epoch(epochId: 1) {
    totalTransactions
    transactions( first: 3, filter: { atCheckpoint: 2 }) {
      edges { cursor node { ...TransactionFragment } } 
    }
  }
}

fragment TransactionFragment on Transaction {
  digest
  effects { checkpoint { sequenceNumber, epoch { epochId } } }
}