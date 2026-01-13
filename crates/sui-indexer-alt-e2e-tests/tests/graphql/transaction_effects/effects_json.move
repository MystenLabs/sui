// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# programmable --sender A --inputs 1000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Test effectsJson field - basic query
  transactionEffects(digest: "@{digest_1}") {
    effectsJson
  }
}

//# run-graphql
{ # Test effectsJson through transaction.effects
  transaction(digest: "@{digest_1}") {
    digest
    effects {
      effectsJson
    }
  }
}

//# run-graphql
{ # Test effectsJson for non-existent transaction
  transactionEffects(digest: "11111111111111111111111111111111") {
    effectsJson
  }
}
