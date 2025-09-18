// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# run-graphql
{ # no fund outflow initially
  e0: epoch(epochId: 0) {
    fundOutflow
  }
}

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# run-graphql
{ # no fund outflow before epoch end
  e0: epoch(epochId: 0) {
    fundOutflow
  }
}

//# advance-epoch

//# run-graphql
{ # fund outflow after epoch end
  e0: epoch(epochId: 0) {
    fundOutflow
  }
}