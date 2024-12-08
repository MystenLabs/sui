// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 A=0x42 --simulator --epochs-to-keep 2 --objects-snapshot-min-checkpoint-lag 1

//# publish
module Test::M1 {
    use sui::coin::Coin;

    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    fun foo<T: key, T2: drop>(_p1: u64, value1: T, _value2: &Coin<T2>, _p2: u64): T {
        value1
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }
}

//# run Test::M1::create --args 0 @A

//# create-checkpoint

//# advance-epoch

//# run Test::M1::create --args 1 @A

//# create-checkpoint

//# advance-epoch

//# run Test::M1::create --args 2 @A

//# create-checkpoint

//# advance-epoch

//# run-graphql --wait-for-checkpoint-pruned 4
{
  epoch {
    epochId
  }
  checkpoints {
    nodes {
      epoch {
        epochId
      }
      sequenceNumber
    }
  }
}

//# run-graphql
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
}

//# run-graphql
{
    chainIdentifier
}

//# run-graphql
{
  epoch(id: 0) {
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
