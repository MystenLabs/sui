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
// Then have objects snapshot processor update `objects_snapshot` so that the available range is between checkpoints 7 and 11.
// The object should still be accessible through point lookups at all versions.

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator --objects-snapshot-min-checkpoint-lag 5

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

//# run Test::M1::create --args 0 @A

//# create-checkpoint

//# run Test::M1::create --args 0 @A

//# create-checkpoint

//# run Test::M1::create --args 0 @A

//# create-checkpoint

//# run Test::M1::create --args 0 @A

//# create-checkpoint

//# run Test::M1::create --args 0 @A

//# create-checkpoint

//# run Test::M1::create --args 0 @A

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql
# Querying objects by version doesn't require it to be in the snapshot table.
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  indexed_object: object(
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
  wrapped_or_deleted_object: object(
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

//# run-graphql --cursors bcs(@{obj_1_0},6)
# But it would no longer be possible to try to paginate using a cursor that falls outside the available range
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  objects(after: "@{cursor_0}") {
    nodes {
      asMoveObject {
        contents {
          json
        }
      }
    }
  }
}
