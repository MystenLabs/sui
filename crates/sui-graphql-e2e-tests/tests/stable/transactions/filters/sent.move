// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator

//# programmable --sender A --inputs 1000000 @B
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender B --inputs 2000000 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
query {
  bySentAddress: transactionBlocks(filter: { sentAddress: "@{A}" }) {
    nodes { ...CoinBalances }
  }

  compoundBySentAddress: transactionBlocks(filter: { sentAddress: "@{A}", kind: PROGRAMMABLE_TX }) {
    nodes { ...CoinBalances }
  }

  sentViaAddress: address(address: "@{A}") {
    transactionBlocks(relation: SENT) {
      nodes { ...CoinBalances }
    }
  }
}

fragment CoinBalances on TransactionBlock {
  effects {
    objectChanges {
      nodes {
        inputState { ...CoinBalance }
        outputState { ...CoinBalance }
      }
    }
  }
}

fragment CoinBalance on Object {
  asMoveObject { asCoin { coinBalance } }
}
