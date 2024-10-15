// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Check that if the sender and gas sponsor of a tx are the same, that the historical data returned
// is also the same. Check that the data for an object at version is consistent. Verify that when
// objects_snapshot has caught up to [0, 3), we can still see all objects since all are live.


// cp | version
// ------------
// 1  | (2)
// 2  | (3, 3, 3)
// 3  | (4, 4, 4, 4)

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator

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
  latest_tx_at_checkpoint_3: transactionBlocks(last: 1, filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        objects_consistent_with_address_at_latest_checkpoint_4: objects(filter: {type: "@{Test}"}) {
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
    }
  }
  object_at_checkpoint_2: object(address: "@{obj_2_0}", version: 3) {
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

//# run-graphql
{
  all_transactions: transactionBlocks(first: 4, filter: {sentAddress: "@{A}"}) {
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
    }
  }
}

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
  latest_tx_at_checkpoint_3: transactionBlocks(last: 1, filter: {sentAddress: "@{A}"}) {
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
    }
  }
  object_at_checkpoint_2: object(address: "@{obj_2_0}", version: 3) {
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

//# create-checkpoint

//# run-graphql
# Regardless of the transaction block, the nested fields should yield the same data.
{
  all_transactions: transactionBlocks(first: 4, filter: {sentAddress: "@{A}"}) {
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
      }
    }
  }
}
