// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# programmable --sender A --inputs 1000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Test transactionJson field - basic query
  transaction(digest: "@{digest_1}") {
    transactionJson
  }
}

//# run-graphql
{ # Test transactionJson through effects.transaction
  transactionEffects(digest: "@{digest_1}") {
    transaction {
      transactionJson
    }
  }
}

//# run-graphql
{ # Test transactionJson for non-existent transaction
  transaction(digest: "11111111111111111111111111111111") {
    transactionJson
  }
}
