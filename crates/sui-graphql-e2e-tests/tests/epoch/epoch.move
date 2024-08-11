// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --simulator --accounts C


// TODO: Short term hack to get around indexer epoch issue
//# create-checkpoint

//# advance-epoch

//# programmable --sender C --inputs 10000000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# run 0x3::sui_system::request_add_stake --args object(0x5) object(3,0) @validator_0 --sender C

// TODO: Short term hack to get around indexer epoch issue
//# create-checkpoint

//# advance-epoch

// run another transaction
//# programmable --sender C --inputs 10000000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// TODO: Short term hack to get around indexer epoch issue
//# create-checkpoint

//# advance-epoch
// check the epoch metrics
//# run-graphql
{
  epoch(id: 2) {
    validatorSet {
      totalStake
      activeValidators {
        nodes {
          name
        }
      }
      validatorCandidatesSize
      inactivePoolsId
    }
    totalGasFees
    totalStakeRewards
    totalStakeSubsidies
    fundSize
    fundInflow
    fundOutflow
    netInflow
    transactionBlocks {
      nodes {
        kind {
          __typename
        }
        digest
      }
    }
  }
}
