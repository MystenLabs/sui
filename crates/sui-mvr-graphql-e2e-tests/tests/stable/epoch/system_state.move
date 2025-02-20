// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --simulator --accounts C --custom-validator-account 

// Run a few transactions and check that the system state storage fund is correctly reported
// for historical epochs

// TODO: Short term hack to get around indexer epoch issue
//# create-checkpoint

//# advance-epoch

//# programmable --sender C --inputs 10000000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# run 0x3::sui_system::request_add_stake --args object(0x5) object(3,0) @validator_0 --sender C

//# create-checkpoint

//# advance-epoch

//# create-checkpoint

//# advance-epoch

//# programmable --sender C --inputs 10000000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# advance-epoch

//# run 0x3::sui_system::request_withdraw_stake --args object(0x5) object(4,0) --sender C

//# create-checkpoint

//# advance-epoch

//# programmable --sender C --inputs 10000000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# advance-epoch

//# run-graphql
{
  epoch(id: 4) {
    epochId
    systemStateVersion
    storageFund {
      totalObjectStorageRebates
      nonRefundableBalance
    }
  }
}

//# run-graphql
{
  epoch(id: 3) {
    epochId
    systemStateVersion
    storageFund {
      totalObjectStorageRebates
      nonRefundableBalance
    }
  }
}

//# run-graphql
{
  epoch(id: 2) {
    epochId
    systemStateVersion
    storageFund {
      totalObjectStorageRebates
      nonRefundableBalance
    }
  }
}

//# run-graphql
{
  epoch(id: 1) {
    epochId
    systemStateVersion
    storageFund {
      totalObjectStorageRebates
      nonRefundableBalance
    }
  }
}

//# run-graphql
{
  epoch(id: 4) {
    epochId
    systemStateVersion
    storageFund {
      totalObjectStorageRebates
      nonRefundableBalance
    }
  } 
}
