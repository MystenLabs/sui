// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# run-graphql
{ # no net inflow initially
  e0: epoch(epochId: 0) {
    netInflow
  }
}

//# programmable --sender A --inputs 42 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 42 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# run-graphql
{ # no net inflow before epoch end
  e0: epoch(epochId: 0) {
    netInflow
  }
}

//# advance-epoch

//# run-graphql
{ # net inflow after epoch end
  e0: epoch(epochId: 0) {
    netInflow
  }
}