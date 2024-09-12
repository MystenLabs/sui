// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 A=0x42 --simulator --epochs-to-keep 1

//# publish
module Test::M1 {
    public struct Object has key, store {
        id: UID,
        value: u64,
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

//# run-graphql
{
  object(address: "@{obj_2_0}", version: 3) {
    digest
  }
}

//# run-graphql
{
  address(address: "@{A}") {
    objects {
      nodes {
        digest
      }
    }
  }
}

//# create-checkpoint

//# advance-epoch

# We must create a checkpoint so that available checkpoint range is still valid after pruning.
//# create-checkpoint

# After pruning, we can still read the object, but won't be able to read indexed data such as address owned objects.
//# run-graphql --wait-for-checkpoint-pruned 0
{
  object(address: "@{obj_2_0}", version: 3) {
    digest
  }
}

//# run-graphql
{
  address(address: "@{A}") {
    objects {
      nodes {
        digest
      }
    }
  }
}
