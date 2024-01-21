// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The first PTB creates a single object that will be modified in subsequent txs. The second creates
// 2, and the third creates 3. Query for the last transaction signed by A, then the second last, and
// the first one. Validate that these return the same set of objects as a query for object(version)
// { address { objects } }.

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

    public entry fun set_value(o: &mut Object, value: u64) {
        o.value = value;
    }
}

//# programmable --sender A --inputs 0 1 @A
//> 0: Test::M1::create(Input(0), Input(2));

//# create-checkpoint

//# programmable --sender A --inputs object(2,0) 100 2 3 @A
//> Test::M1::set_value(Input(0), Input(1));
//> Test::M1::create(Input(2), Input(4));
//> Test::M1::create(Input(3), Input(4));

//# create-checkpoint

//# programmable --sender A --inputs object(2,0) 200 4 5 6 @A
//> Test::M1::set_value(Input(0), Input(1));
//> Test::M1::create(Input(2), Input(5));
//> Test::M1::create(Input(3), Input(5));
//> Test::M1::create(Input(4), Input(5));

//# create-checkpoint

//# run-graphql
# There should be 6 objects under sender at the latest checkpoint.
{
  transactionBlocks(last: 1, filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
        objects(filter: {type: "@{Test}"}) {
          nodes {
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
  }
}

//# run-graphql
# This query should yield the same result.
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}) {
      nodes {
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
# Do the same thing from the object's point of view, starting from the latest version.
{
  object(address: "@{obj_2_0}") {
    owner {
      ... on AddressOwner {
        owner {
          objects(filter: {type: "@{Test}"}) {
            nodes {
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
    }
  }
}

//# run-graphql --cursors 4
# View the second PTB, which resulted in a total of 3 objects owned by sender.
{
  transactionBlocks(last: 1, before: "@{cursor_0}", filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
        objects(filter: {type: "@{Test}"}) {
          nodes {
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
  }
}

//# run-graphql
# Using the object that has been modified at every checkpoint, view the state at the second-to-last mutation.
{
  object(address: "@{obj_2_0}", version: 3) {
    owner {
      ... on AddressOwner {
        owner {
          objects(filter: {type: "@{Test}"}) {
            nodes {
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
    }
  }
}

//# run-graphql --cursors 3
# View the first PTB, which created 1 object.
{
  transactionBlocks(last: 1, before: "@{cursor_0}", filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
        objects(filter: {type: "@{Test}"}) {
          nodes {
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
  }
}

//# run-graphql
# Using the object that has been modified at every checkpoint, view the state at its first version.
{
  object(address: "@{obj_2_0}", version: 2) {
    owner {
      ... on AddressOwner {
        owner {
          objects(filter: {type: "@{Test}"}) {
            nodes {
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
    }
  }
}
