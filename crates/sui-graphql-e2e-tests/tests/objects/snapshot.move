// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Objects can continue to be found on the live objects table until they are WrappedOrDeleted. From
// there, the object can be fetched on the objects_history table, until it gets snapshotted into
// objects_snapshot table. This test checks that we correctly fetch from objects_snapshot, by
// creating an object at checkpoint 1, wrapping it at checkpoint 2, and progressing checkpoints
// until the lag exceeds the difference between max and min lag. At this point, checkpoint 2 and its
// contents will be committed to objects_snapshot. The first indexing occurs at checkpoint 3 as 3 >
// 2 - 0 + 0 (start_cp), and we index checkpoints [0, 0 + 2 - 0), so objects_snapshot is now at
// checkpoint 1. The next snapshot occurs at checkpoint 4, as 4 > 2 - 0 + 1 (start_cp), and we index
// checkpoints [1, 1 + 2 - 0), so objects_snapshot is now at checkpoint 2.

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

//# run-graphql
# should not exist on live objects
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
