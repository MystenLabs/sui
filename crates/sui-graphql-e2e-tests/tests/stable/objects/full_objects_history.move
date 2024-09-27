// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --simulator --epochs-to-keep 1

//# publish
module Test::M1 {
    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public entry fun create(value: u64, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            ctx.sender(),
        )
    }

    public entry fun mutate(obj: &mut Object, value: u64) {
        obj.value = value;
    }
}

//# run Test::M1::create --args 0

//# create-checkpoint

//# advance-epoch

//# run Test::M1::mutate --args object(2,0) 1

//# create-checkpoint

//# run-graphql
{
  object(address: "@{obj_2_0}", version: 3) {
    digest
  }
}

//# run-graphql
{
  object(address: "@{obj_2_0}", version: 4) {
    digest
  }
}

//# advance-epoch

//# create-checkpoint

# After pruning, we can still read all historical objects.
//# run-graphql --wait-for-checkpoint-pruned 0
{
  object(address: "@{obj_2_0}", version: 3) {
    digest
  }
}
