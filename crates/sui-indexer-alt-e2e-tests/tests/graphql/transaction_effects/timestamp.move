// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// Test system transaction created by advance-clock
//# advance-clock --duration-ns 1000000

//# create-checkpoint

//# run-graphql
{ # Test timestamp field on transfer transaction
  transferTransaction: transactionEffects(digest: "@{digest_1}") {
    timestamp
  }
}

//# run-graphql
{ # Test timestamp field on system transaction
  systemTransaction: transactionEffects(digest: "@{digest_3}") {
    timestamp
  }
}
