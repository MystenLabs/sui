// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --simulator --accounts C

//# advance-epoch

//# programmable --sender C --inputs 10000000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# run 0x3::sui_system::request_add_stake --args object(0x5) object(2,0) @validator_0 --sender C

//# advance-epoch

// run another transaction
//# programmable --sender C --inputs 10000000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# advance-epoch
// check the epoch metrics
//# run-graphql
{
  epoch(id: 2) {
    validatorSet {
      totalStake
    }
    totalGasFees
    totalStakeRewards
    totalStakeSubsidies
    fundSize
    fundInflow
    fundOutflow
    netInflow
  }
}
