// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Transfer 500 objects to A. The first graphql query fetches the last 4 objects owned by A. Then
// transfer the last 3 objects from A to B. Make a graphql query for the `last: 1` - this is to test
// that we return the next valid result even if the first `limit` rows that match the filtering
// criteria are then invalidated by a newer version of the matched object. We set `last: 1` but
// transfer the last 3 objects because we increase the limit by 2 behind the scenes.

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator

//# publish
module Test::M1 {
    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public struct Ledger has key, store {
        id: UID,
        object_ids: vector<UID>,
    }

    public entry fun create_many(recipient: address, ctx: &mut TxContext) {
        let mut i = 0;
        while (i < 500) {
            transfer::public_transfer(

                Object { id: object::new(ctx), value: i },
                recipient
            );
            i = i + 1;
        }
    }
}

//# run Test::M1::create_many --sender A --args @A

//# create-checkpoint 2

//# run-graphql
{
  last_2: objects(last: 2, filter: {type: "@{Test}"}) {
    nodes {
      version
      asMoveObject {
        owner {
          ... on AddressOwner {
            owner {
              address
            }
          }
        }
        contents {
          json
          type {
            repr
          }
        }
      }
    }
  }
  last_4_objs_owned_by_A: address(address: "@{A}") {
    objects(last: 4) {
      nodes {
        owner {
          ... on AddressOwner {
            owner {
              address
            }
          }
        }
        contents {
          json
          type {
            repr
          }
        }
      }
    }
  }
}

//# transfer-object 2,499 --sender A --recipient B

//# transfer-object 2,498 --sender A --recipient B

//# transfer-object 2,497 --sender A --recipient B

//# view-object 2,498

//# view-object 2,497

//# create-checkpoint

//# run-graphql
{
  last_3: objects(last: 3, filter: {type: "@{Test}"}) {
    nodes {
      version
      asMoveObject {
        owner {
          ... on AddressOwner {
            owner {
              address
            }
          }
        }
        contents {
          json
          type {
            repr
          }
        }
      }
    }
  }
  last_obj_owned_by_A: address(address: "@{A}") {
    objects(last: 1) {
      nodes {
        version
        owner {
          ... on AddressOwner {
            owner {
              address
            }
          }
        }
        contents {
          json
          type {
            repr
          }
        }
      }
    }
  }
}

//# run-graphql
# Test that we correctly return the object at version, both for the `object` and `objects`
# resolvers.
{
  a: object(address: "@{obj_2_499}", version: 2) {
    asMoveObject {
      owner {
        ... on AddressOwner {
          owner {
            address
          }
        }
      }
      contents {
        json
        type {
          repr
        }
      }
    }
  }
  b: object(address: "@{obj_2_499}", version: 3) {
    asMoveObject {
      owner {
        ... on AddressOwner {
          owner {
            address
          }
        }
      }
      contents {
        json
        type {
          repr
        }
      }
    }
  }
  objects_a: objects(filter: {objectKeys: [
    {objectId: "@{obj_2_499}", version: 2},
    {objectId: "@{obj_2_498}", version: 2},
    {objectId: "@{obj_2_497}", version: 2},
    ]}) {
    nodes {
      asMoveObject {
        owner {
          ... on AddressOwner {
            owner {
              address
            }
          }
        }
        contents {
          json
          type {
            repr
          }
        }
      }
    }
  }
  objects_b: objects(filter: {objectKeys: [
    {objectId: "@{obj_2_499}", version: 3},
    {objectId: "@{obj_2_498}", version: 4},
    {objectId: "@{obj_2_497}", version: 5},
    ]}) {
    nodes {
      asMoveObject {
        owner {
          ... on AddressOwner {
            owner {
              address
            }
          }
        }
        contents {
          json
          type {
            repr
          }
        }
      }
    }
  }
  owned_by_b: address(address: "@{B}") {
    objects {
      nodes {
        version
        owner {
          ... on AddressOwner {
            owner {
              address
            }
          }
        }
        contents {
          json
          type {
            repr
          }
        }
      }
    }
  }
}
