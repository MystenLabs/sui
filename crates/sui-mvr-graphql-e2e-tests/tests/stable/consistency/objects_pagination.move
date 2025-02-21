// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator --objects-snapshot-min-checkpoint-lag 4


// cp | object_id | owner
// ----------------------
// 1  | obj_2_0   | A
// 1  | obj_3_0   | A
// 2  | obj_6_0   | A
// 2  | obj_7_0   | A
// All owned by B after checkpoint 3.

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

//# run Test::M1::create --args 1 @A

//# create-checkpoint

//# run-graphql --cursors bcs(@{obj_2_0},@{highest_checkpoint}) bcs(@{obj_3_0},@{highest_checkpoint})
{
  one_of_these_will_yield_an_object: address(address: "@{A}") {
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
  if_the_other_does_not: objects(filter: {type: "@{Test}"}, after: "@{cursor_1}") {
    nodes {
      version
      asMoveObject {
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

//# run-graphql --cursors bcs(@{obj_2_0},1) bcs(@{obj_3_0},1)
{
  paginating_on_checkpoint_1: address(address: "@{A}") {
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
  should_not_have_more_than_one_result: objects(filter: {type: "@{Test}"}, after: "@{cursor_1}") {
    nodes {
      version
      asMoveObject {
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
{
  four_objects: address(address: "@{A}") {
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
{
  objects_at_version: address(address: "@{A}") {
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

//# run-graphql --cursors bcs(@{obj_6_0},2)
{
  after_obj_6_0_at_checkpoint_2: address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}, after: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
        owner_at_latest_state_has_sui_only: owner {
          ... on AddressOwner {
            owner {
              objects {
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
        }
      }
    }
  }
  before_obj_6_0_at_checkpoint_2: objects(filter: {type: "@{Test}"}, before: "@{cursor_0}") {
    nodes {
      version
      asMoveObject {
        contents {
          type {
            repr
          }
          json
        }
        note_that_owner_result_should_reflect_latest_state: owner {
          ... on AddressOwner {
            owner {
              objects {
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
        }
      }
    }
  }
}

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql --cursors bcs(@{obj_6_0},2)
# This query will error due to requesting data outside of available range
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  before_obj_6_0_at_checkpoint_2: objects(filter: {type: "@{Test}"}, before: "@{cursor_0}") {
    nodes {
      version
      asMoveObject {
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
  owned_by_address_b_latest: address(address: "@{B}") {
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
# Historical lookups will still return results at version.
{
  objects_at_version: address(address: "@{A}") {
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
