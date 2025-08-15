// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# run-graphql
{
  epoch0BeforeCp0NotTx: epoch(epochId: 0) {
    totalTransactions
    transactions( first: 3, filter: { beforeCheckpoint: 0 }) {
      edges { cursor node { digest, sender { address } } } 
    }
  }
}

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# advance-epoch

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 43 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  epoch0BeforeCp1: epoch(epochId: 0) {
    totalTransactions
    transactions( first: 3, filter: { beforeCheckpoint: 1 }) {
      edges { cursor node { digest effects { checkpoint { sequenceNumber, epoch { epochId } } } } }
    }
  }
}

//# advance-epoch

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# programmable --sender A --inputs 2
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# run-graphql
{
  epoch1BeforeCp5: epoch(epochId: 1) {
    totalTransactions
    transactions( first: 3, filter: { beforeCheckpoint: 5 }) {
      edges { cursor node { digest effects { checkpoint { sequenceNumber, epoch { epochId } } } } }
    }
  }
}

//# run-graphql
{
  epoch1BeforeCp0: epoch(epochId: 1) {
    totalTransactions
    transactions( first: 3, filter: { beforeCheckpoint: 1 }) {
      edges { cursor node { digest effects { checkpoint { sequenceNumber, epoch { epochId } } } } }
    }
  }
}