// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Simple test suite to validate expected results from reading objects_history. objects_snapshot
// remains empty for this test, so we expect the start_cp to coalesce to 0. Every time we modify an
// object, we create a checkpoint to extend end_cp. Create an object, update, wrap, unwrap, and
// finally delete it. When the object is WrappedOrDeleted, it should not be findable on the live
// objects table, but can still be found on the objects_history table. Verify that the object's
// previous versions are retrievable.

//# init --addresses Test=0x0 --accounts A --simulator

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

    public entry fun update(o1: &mut Object, value: u64,) {
        o1.value = value;
    }

    public entry fun wrap(o: Object, ctx: &mut TxContext) {
        transfer::transfer(Wrapper { id: object::new(ctx), o }, tx_context::sender(ctx))
    }

    public entry fun unwrap(w: Wrapper, ctx: &mut TxContext) {
        let Wrapper { id, o } = w;
        object::delete(id);
        transfer::public_transfer(o, tx_context::sender(ctx))
    }

    public entry fun delete(o: Object) {
        let Object { id, value: _ } = o;
        object::delete(id);
    }
}

//# run Test::M1::create --args 0 @A

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

//# run Test::M1::update --sender A --args object(2,0) 1

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

//# run Test::M1::unwrap --sender A --args object(9,0)

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
{
  object(
    address: "@{obj_2_0}"
    version: 5
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

//# run Test::M1::delete --sender A --args object(2,0)

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
{
  object(
    address: "@{obj_2_0}"
    version: 7
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
    version: 6
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
    version: 5
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
