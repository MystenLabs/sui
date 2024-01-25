// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


// Verify that an object can be retrieved at previous versions and when WrappedOrDeleted. Increment
// objects_snapshot and verify that objects at versions beyond the available range return a null
// result.

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
# Version should increment.
{
  latest_version: object(
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
  previous_version: object(
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
# Wrapped object should still be available.
{
  latest_wrapped: object(
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
  previous_version: object(
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

//# run Test::M1::unwrap --sender A --args object(2,0)

//# create-checkpoint

//# run-graphql
# Unwrapping an object should allow its contents to be visible again.
{
  latest_unwrapped: object(
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
  previous_version: object(
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
  first_version: object(
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
# Object is now deleted.
{
  latest_deleted: object(
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
  version_specified: object(
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

//# force-object-snapshot-catchup --start-cp 0 --end-cp 5

//# run-graphql
{
  object_within_available_range: object(
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
  object_outside_available_range: object(
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
