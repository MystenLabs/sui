// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# programmable --sender A --inputs 42 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender B --inputs 43 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  a: address(address: "@{A}") { address }
  b: address(address: "@{B}") { address }

  transactionA: transaction(digest: "@{digest_1}") {
    sender { address }
  }

  transactionB: transaction(digest: "@{digest_2}") {
    sender { address }
  }
}
