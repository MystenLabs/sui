// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --accounts A B --simulator

//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Test version field exposes the effects schema discriminator (V1 -> 1, V2 -> 2)
  transaction: transactionEffects(digest: "@{digest_1}") {
    version
  }
}
