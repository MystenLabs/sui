// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator

// Confirm that the new `sentAddress` behaves like `signAddress`, and how the
// two filters interact with each other.

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

  bySignAddress: transactionBlocks(filter: { signAddress: "@{A}" }) {
    nodes { ...CoinBalances }
  }

  bothAddresses: transactionBlocks(filter: { sentAddress: "@{A}", signAddress: "@{A}" }) {
    nodes { ...CoinBalances }
  }

  differentAddresses: transactionBlocks(filter: { sentAddress: "@{A}", signAddress: "@{B}" }) {
    nodes { ...CoinBalances }
  }

  compoundBySentAddress: transactionBlocks(filter: { sentAddress: "@{A}", kind: PROGRAMMABLE_TX }) {
    nodes { ...CoinBalances }
  }

  compoundBySignAddress: transactionBlocks(filter: { signAddress: "@{A}", kind: PROGRAMMABLE_TX }) {
    nodes { ...CoinBalances }
  }

  compoundBothAddresses: transactionBlocks(filter: {
    sentAddress: "@{A}",
    signAddress: "@{A}",
    kind: PROGRAMMABLE_TX,
  }) {
    nodes { ...CoinBalances }
  }

  compoundDifferentAddresses: transactionBlocks(filter: {
    sentAddress: "@{A}",
    signAddress: "@{B}",
    kind: PROGRAMMABLE_TX,
  }) {
    nodes { ...CoinBalances }
  }

  sentViaAddress: address(address: "@{A}") {
    transactionBlocks(relation: SENT) {
      nodes { ...CoinBalances }
    }
  }

  signViaAddress: address(address: "@{A}") {
    transactionBlocks(relation: SIGN) {
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
