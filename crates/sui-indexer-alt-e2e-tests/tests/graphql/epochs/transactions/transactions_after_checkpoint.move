// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# run-graphql
{
  epoch0AfterCp0NoTx: epoch(epochId: 0) {
    totalTransactions
    transactions(first: 3, filter: { afterCheckpoint: 0 }) {
      edges { cursor node { digest effects { checkpoint { sequenceNumber, epoch { epochId } } } } }
    }
  }
}

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  epoch0AfterCp0WithTx: epoch(epochId: 0) {
    totalTransactions
    transactions(first: 3, filter: { afterCheckpoint: 0 }) {
      edges { cursor node { digest effects { checkpoint { sequenceNumber, epoch { epochId } } } } }
    }
  }
}

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  epoch0AfterCp1WithTx: epoch(epochId: 0) {
    totalTransactions
    transactions(first: 3, filter: { afterCheckpoint: 1 }) {
      edges { cursor node { digest effects { checkpoint { sequenceNumber, epoch { epochId } } } } }
    }
  }
}

//# advance-epoch

//# programmable --sender A --inputs 43 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{
  epoch1AfterCp2: epoch(epochId: 1) {
    totalTransactions
    transactions(first: 3, filter: { afterCheckpoint: 2 }) {
      edges { cursor node { digest effects { checkpoint { sequenceNumber, epoch { epochId } } } } }
    }
  }
  epoch1AfterNonExistentCp5: epoch(epochId: 1) {
    totalTransactions
    transactions(first: 3, filter: { afterCheckpoint: 5 }) {
      edges { node { digest effects { checkpoint { sequenceNumber, epoch { epochId } } } } }
    }
  }
}

//# create-checkpoint
