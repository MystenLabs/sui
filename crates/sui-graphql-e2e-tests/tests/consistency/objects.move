// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --accounts A B --simulator

//# publish
module Test::M1 {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use sui::transfer;

    struct Object has key, store {
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

//# run Test::M1::create --args 1 @A

//# create-checkpoint

//# run-graphql
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}) {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
}

//# run Test::M1::create --args 2 @A

//# run Test::M1::create --args 3 @A

//# create-checkpoint

//# run-graphql --cursors @{obj_2_0,1}
# We should see one or no objects, depending on how the object_ids are ordered
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}, after: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
}

//# run-graphql --cursors @{obj_3_0,1}
# Thus we also make this query - if the previous query yielded no results, this should yield one
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}, after: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
}

//# run-graphql
# The query for live objects should show 4 objects
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}) {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
}

//# run-graphql
# Selecting objects at version should also yield results
{
  address(address: "@{A}") {
    objects(
      filter: {
        type: "@{Test}",
        objectKeys: [
            {objectId: "@{obj_2_0}", version: 3},
            {objectId: "@{obj_3_0}", version: 4},
            {objectId: "@{obj_6_0}", version: 5},
            {objectId: "@{obj_7_0}", version: 6}
            ]
      }
    ) {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
}

//# programmable --sender A --inputs object(2,0) object(3,0) object(6,0) object(7,0) @B
//> TransferObjects([Input(0), Input(1), Input(2), Input(3)], Input(4))

//# create-checkpoint

//# run-graphql --cursors @{obj_6_0,2}
# We should see objects - the cursor should still be valid.
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}, after: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
}

//# run-graphql
# Should not have any objects on the live objects table
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}) {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
}

//# run-graphql
# This one should also yield no results, since there are more recent versions of the object at the checkpo
{
  address(address: "@{A}") {
    objects(
      filter: {
        type: "@{Test}",
        objectKeys: [
            {objectId: "@{obj_2_0}", version: 3},
            {objectId: "@{obj_3_0}", version: 4},
            {objectId: "@{obj_6_0}", version: 5},
            {objectId: "@{obj_7_0}", version: 6}
            ]
      }
    ) {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
}

//# run-graphql
# Should have all the objects
{
  address(address: "@{B}") {
    objects(filter: {type: "@{Test}"}) {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
}
