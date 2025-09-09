// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# programmable --sender A --inputs 42 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 44 @B --expiration 10
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Null expiration
  transaction(digest: "@{digest_1}") {
    expiration {
      epochId
    }
  }
}

//# run-graphql
{ # Non-Null expiration
  transaction(digest: "@{digest_2}") {
    expiration {
      epochId
      startTimestamp
      endTimestamp
    }
  }
}
