// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

<<<<<<< HEAD
// Check that if the sender and gas sponsor of a tx are the same, that the historical data returned
// is also the same. Check that the data for an object at version is consistent. Verify that when
// objects_snapshot has caught up to [0, 3), we can still see all objects since all are live.


// cp | version
// ------------
// 1  | (2)
// 2  | (3, 3, 3)
// 3  | (4, 4, 4, 4)
=======
// The first PTB creates a single object that will be modified in subsequent txs. The second creates
// 2, and the third creates 3. Query for the last transaction signed by A, then the second last, and
// the first one. Validate that these return the same set of objects as a query for object(version)
// { address { objects } }.
>>>>>>> 6667271423 (Consistent reads what the last PR should look like)

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

<<<<<<< HEAD
//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql
{
  address_at_latest_checkpoint_4: address(address: "@{A}") {
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
  latest_tx_at_checkpoint_3: transactionBlocks(last: 1, filter: {signAddress: "@{A}"}) {
=======
//# run-graphql
# There should be 6 objects under sender at the latest checkpoint.
{
  transactionBlocks(last: 1, filter: {signAddress: "@{A}"}) {
>>>>>>> 6667271423 (Consistent reads what the last PR should look like)
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
<<<<<<< HEAD
      gasInput {
        gasSponsor {
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
        gasPayment {
          nodes {
            contents {
              type {
                repr
              }
              json
            }
=======
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
>>>>>>> 6667271423 (Consistent reads what the last PR should look like)
          }
        }
      }
    }
  }
<<<<<<< HEAD
  object_at_checkpoint_2: object(address: "@{obj_2_0}", version: 3) {
=======
}

//# run-graphql
# Do the same thing from the object's point of view, starting from the latest version.
{
  object(address: "@{obj_2_0}") {
>>>>>>> 6667271423 (Consistent reads what the last PR should look like)
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

<<<<<<< HEAD
//# run-graphql
{
  all_transactions: transactionBlocks(first: 4, filter: {signAddress: "@{A}"}) {
=======
//# run-graphql --cursors 4
# View the second PTB, which resulted in a total of 3 objects owned by sender.
{
  transactionBlocks(last: 1, before: "@{cursor_0}", filter: {signAddress: "@{A}"}) {
>>>>>>> 6667271423 (Consistent reads what the last PR should look like)
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
<<<<<<< HEAD
      gasInput {
        gasSponsor {
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
        gasPayment {
          nodes {
            contents {
              type {
                repr
              }
              json
            }
          }
        }
      }
=======
>>>>>>> 6667271423 (Consistent reads what the last PR should look like)
    }
  }
}

<<<<<<< HEAD
//# force-object-snapshot-catchup --start-cp 0 --end-cp 3

//# run-graphql
{
  address_at_latest_checkpoint_4: address(address: "@{A}") {
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
  latest_tx_at_checkpoint_3: transactionBlocks(last: 1, filter: {signAddress: "@{A}"}) {
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
      gasInput {
        gasSponsor {
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
        gasPayment {
          nodes {
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
  object_at_checkpoint_2: object(address: "@{obj_2_0}", version: 3) {
=======
//# run-graphql
# Using the object that has been modified at every checkpoint, view the state at the second-to-last mutation.
{
  object(address: "@{obj_2_0}", version: 3) {
>>>>>>> 6667271423 (Consistent reads what the last PR should look like)
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

<<<<<<< HEAD
//# run-graphql
# First transaction should have no results since its data is no longer in objects_snapshot. Second
# transaction is still valid - since objects_snapshot table is at [0, 3), it will have data from
# checkpoint 2.
{
  all_transactions: transactionBlocks(first: 4, filter: {signAddress: "@{A}"}) {
=======
//# run-graphql --cursors 3
# View the first PTB, which created 1 object.
{
  transactionBlocks(last: 1, before: "@{cursor_0}", filter: {signAddress: "@{A}"}) {
>>>>>>> 6667271423 (Consistent reads what the last PR should look like)
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
<<<<<<< HEAD
      gasInput {
        gasSponsor {
=======
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
>>>>>>> 6667271423 (Consistent reads what the last PR should look like)
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
<<<<<<< HEAD
        gasPayment {
          nodes {
            contents {
              type {
                repr
              }
              json
            }
          }
        }
=======
>>>>>>> 6667271423 (Consistent reads what the last PR should look like)
      }
    }
  }
}
