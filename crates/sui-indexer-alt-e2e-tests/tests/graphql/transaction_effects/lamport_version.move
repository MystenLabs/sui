// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 200 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Test lamport_version field progression across multiple transactions using the same gas coin
  firstTransaction: transactionEffects(digest: "@{digest_1}") {
    lamportVersion
  }
  
  secondTransaction: transactionEffects(digest: "@{digest_2}") {
    lamportVersion
  }
} 
