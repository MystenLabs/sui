// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --simulator --accounts A

//# programmable --sender A --inputs 1 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs 1 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs 1 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs 1 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs 1 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  transactionBlockConnection(first: 2, after: "1") {
    edges {
      cursor
    }
  }
}

//# run-graphql
{
  transactionBlockConnection(last: 3, before: "3") {
    edges {
      cursor
    }
  }
}

//# run-graphql
{
  transactionBlockConnection(last: 2, before: "3", filter: {atCheckpoint: 2}) {
    edges {
      cursor
    }
  }
}

//# run-graphql
{
  transactionBlockConnection(
    last: 4
    before: "4"
    filter: {afterCheckpoint: 0, beforeCheckpoint: 3}
  ) {
    edges {
      cursor
    }
  }
}
