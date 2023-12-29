// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Objects can continue to be found on the live objects table until they are WrappedOrDeleted. From
// there, the object can be fetched on the objects_history table, until it gets snapshotted into
// objects_snapshot table. This test checks that we correctly fetch data from both the
// objects_snapshot and objects_history tables.

//# init --addresses Test=0x0 --accounts A --simulator --object-snapshot-min-checkpoint-lag 0 --object-snapshot-max-checkpoint-lag 2

//# publish
module Test::M1 {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

    struct Object has key, store {
        id: UID,
        value: u64,
    }

    struct Wrapper has key {
        id: UID,
        o: Object
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }

    public entry fun wrap(o: Object, ctx: &mut TxContext) {
        transfer::transfer(Wrapper { id: object::new(ctx), o }, tx_context::sender(ctx))
    }
}

//# run Test::M1::create --args 0 @A

//# create-checkpoint 1

//# run-graphql
{
  object(
    address: "@{obj_2_0}"
  ) {
    status
    version
    asMoveObject {
      contents {
        json
      }
    }
  }
}


//# run-graphql
{
  object(
    address: "@{obj_2_0}"
    version: 3
  ) {
    status
    version
    asMoveObject {
      contents {
        json
      }
    }
  }
}

//# run Test::M1::wrap --sender A --args object(2,0)

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql
{
  object(
    address: "@{obj_2_0}"
  ) {
    status
    version
    asMoveObject {
      contents {
        json
      }
    }
  }
}


//# run-graphql
# fetched from objects_snapshot
{
  object(
    address: "@{obj_2_0}"
    version: 4
  ) {
    status
    version
    asMoveObject {
      contents {
        json
      }
    }
  }
}

//# run-graphql
# should not exist in either objects_snapshot or objects_history
{
  object(
    address: "@{obj_2_0}"
    version: 3
  ) {
    status
    version
    asMoveObject {
      contents {
        json
      }
    }
  }
}
