// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

// Transaction in Epoch 0
//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// Advance to Epoch 1
//# advance-epoch

// Transaction in Epoch 1
//# programmable --sender A --inputs 200 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// Advance to Epoch 2
//# advance-epoch

// Transaction in Epoch 2
//# programmable --sender A --inputs 300 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Test epoch field across multiple epochs
  epoch0Transaction: transactionEffects(digest: "@{digest_1}") {
    epoch {
      epochId
    }
  }

  epoch1Transaction: transactionEffects(digest: "@{digest_4}") {
    epoch {
      epochId
    }
  }

  epoch2Transaction: transactionEffects(digest: "@{digest_7}") {
    epoch {
      epochId
    }
  }
}
