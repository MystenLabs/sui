// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --simulator --accounts A

//# programmable --sender A --inputs 1 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs 2 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs 3 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs 4 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs 5 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  transactionBlockConnection(filter: {sentAddress: "@{A}"}) {
    nodes {
      kind {
        ... on ProgrammableTransactionBlock {
          inputConnection(first: 1) {
            nodes {
              ... on Pure {
                bytes
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  transactionBlockConnection(first: 2, after: "1", filter: {sentAddress: "@{A}"}) {
    nodes {
      kind {
        ... on ProgrammableTransactionBlock {
          inputConnection(first: 1) {
            nodes {
              ... on Pure {
                bytes
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  transactionBlockConnection(last: 3, before: "3") {
    nodes {
      kind {
        ... on ProgrammableTransactionBlock {
          inputConnection(first: 1) {
            nodes {
              ... on Pure {
                bytes
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  transactionBlockConnection(last: 2, before: "3", filter: {atCheckpoint: 2}) {
    nodes {
      kind {
        ... on ProgrammableTransactionBlock {
          inputConnection(first: 1) {
            nodes {
              ... on Pure {
                bytes
              }
            }
          }
        }
      }
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
    nodes {
      kind {
        ... on ProgrammableTransactionBlock {
          inputConnection(first: 1) {
            nodes {
              ... on Pure {
                bytes
              }
            }
          }
        }
      }
    }
  }
}
