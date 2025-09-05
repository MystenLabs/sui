// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs 42 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# run-graphql
{ # genesis epoch
  e0: epoch(epochId: 0) { ...E }
  e1: epoch(epochId: 1) { ...E }
  e2: epoch(epochId: 2) { ...E }
}

fragment E on Epoch {
  totalTransactions
}

//# advance-epoch

//# programmable --sender A --inputs 42 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# run-graphql
{ # epochs without middle epoch
  e0: epoch(epochId: 0) { ...E }
  e1: epoch(epochId: 1) { ...E }
  e2: epoch(epochId: 2) { ...E }
}

fragment E on Epoch {
  totalTransactions
}

//# advance-epoch

//# programmable --sender A --inputs 42 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# run-graphql
{ # epochs with middle epoch
  e0: epoch(epochId: 0) { ...E }
  e1: epoch(epochId: 1) { ...E }
  e2: epoch(epochId: 2) { ...E }
}

fragment E on Epoch {
  totalTransactions
}
