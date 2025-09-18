// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# run-graphql
{ # no stake rewards initially
  e0: epoch(epochId: 0) {
    totalStakeRewards
  }
}

//# programmable --sender A --inputs 42 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# run-graphql
{ # no stake rewards after SplitCoins before epoch has end
  e0: epoch(epochId: 0) {
    totalStakeRewards
  }
}

//# advance-epoch

//# run-graphql
{ # stake rewards after epoch has end
  e0: epoch(epochId: 0) {
    totalStakeRewards
  }
}
