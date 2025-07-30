// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# run-graphql
{ # no storage fund initially
  e0: epoch(epochId: 0) { ...E }
  e1: epoch(epochId: 1) { ...E }
}

fragment E on Epoch {
  storageFund {
    totalObjectStorageRebates
    nonRefundableBalance
  }
}

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# advance-epoch

//# create-checkpoint

//# run-graphql
{ # storage fund on first checkpoint of epoch 1
  e0: epoch(epochId: 0) { ...E }
  e1: epoch(epochId: 1) { ...E }
}

fragment E on Epoch {
  storageFund {
    totalObjectStorageRebates
    nonRefundableBalance
  }
}