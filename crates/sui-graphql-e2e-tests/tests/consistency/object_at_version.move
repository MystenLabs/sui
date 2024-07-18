// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Create an object and modify it at checkpoints as follows:
// cp | version
// ------------
// 1  | 3
// 2  | 4
// 3  | 5
// 4  | 6
// 5  | 7
// Verify that the object is returned in its WrappedOrDeleted or Historical state. Increment
// objects_snapshot to [0, 5). This coalesces objects in objects_snapshot to its verson at
// checkpoint 4. The object would only be visible at version 6 from objects_snapshot, and at version
// 7 from objects_history.

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator

//# publish
module Test::M1 {
    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public struct Wrapper has key {
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

//# run Test::M1::unwrap --sender A --args object(8,0)

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

//# create-checkpoint

//# run-graphql
# Querying objects by version doesn't require it to be in the snapshot table.
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
  object_not_in_snapshot: object(
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
