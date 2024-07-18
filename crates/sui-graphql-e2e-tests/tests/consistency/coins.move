// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// chkpt 1 | chkpt 2 | chkpt 3 | chkpt 4 | chkpt 5 | snapshot [0, 4)
// -----------------------------------------------------------------
// coin1@A | coin1@B | coin1@B | coin1@B | coin1@B | coin1@B
// coin2@A | coin2@B | coin2@B | coin2@B | coin2@B | coin2@B
// coin3@A | coin3@A | coin3@A | coin3@A | coin3@A | coin3@A

//# init --protocol-version 51 --addresses P0=0x0 --accounts A B --simulator

//# publish --sender A
module P0::fake {
    use sui::coin;

    public struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext){
        let (mut treasury_cap, metadata) = coin::create_currency(
            witness,
            2,
            b"FAKE",
            b"",
            b"",
            option::none(),
            ctx,
        );

        let c1 = coin::mint(&mut treasury_cap, 100, ctx);
        let c2 = coin::mint(&mut treasury_cap, 200, ctx);
        let c3 = coin::mint(&mut treasury_cap, 300, ctx);

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
        transfer::public_transfer(c1, tx_context::sender(ctx));
        transfer::public_transfer(c2, tx_context::sender(ctx));
        transfer::public_transfer(c3, tx_context::sender(ctx));
    }
}

//# create-checkpoint

//# programmable --sender A --inputs object(1,5) 100000 object(1,1)
//> 0: sui::coin::mint<P0::fake::FAKE>(Input(0), Input(1));
//> MergeCoins(Input(2), [Result(0)]);

//# create-checkpoint

//# run-graphql
{
  queryCoinsAtLatest: coins(type: "@{P0}::fake::FAKE") {
    edges {
      cursor
      node {
        consistentStateForEachCoin: owner {
          ... on AddressOwner {
            owner {
              address
              coins(type: "@{P0}::fake::FAKE") {
                edges {
                  cursor
                  node {
                    contents {
                      json
                    }
                  }
                }
              }
            }
          }
        }
        contents {
          json
        }
      }
    }
  }
  addressCoins: address(address: "@{A}") {
    coins(type: "@{P0}::fake::FAKE") {
      edges {
        cursor
        node {
          contents {
            json
          }
        }
      }
    }
  }
}

//# run-graphql --cursors @{obj_1_3,1}
{
  queryCoinsAtChkpt1: coins(type: "@{P0}::fake::FAKE", before: "@{cursor_0}") {
    edges {
      cursor
      node {
        consistentStateForEachCoin: owner {
          ... on AddressOwner {
            owner {
              address
              coins(type: "@{P0}::fake::FAKE") {
                edges {
                  cursor
                  node {
                    contents {
                      json
                    }
                  }
                }
              }
            }
          }
        }
        contents {
          json
        }
      }
    }
  }
  queryAddressCoinsAtChkpt1: address(address: "@{A}") {
    coins(type: "@{P0}::fake::FAKE", before: "@{cursor_0}") {
      edges {
        cursor
        node {
          contents {
            json
          }
        }
      }
    }
  }
}

//# transfer-object 1,2 --sender A --recipient B

//# transfer-object 1,3 --sender A --recipient B

//# create-checkpoint

//# run-graphql
{
  queryCoins: coins(type: "@{P0}::fake::FAKE") {
    edges {
      cursor
      node {
        owner {
          ... on AddressOwner {
            owner {
              address
              coins(type: "@{P0}::fake::FAKE") {
                edges {
                  cursor
                  node {
                    contents {
                      json
                    }
                  }
                }
              }
            }
          }
        }
        contents {
          json
        }
      }
    }
  }
  addressCoinsA: address(address: "@{A}") {
    coins(type: "@{P0}::fake::FAKE") {
      edges {
        cursor
        node {
          contents {
            json
          }
        }
      }
    }
  }
  addressCoinsB: address(address: "@{B}") {
    coins(type: "@{P0}::fake::FAKE") {
      edges {
        cursor
        node {
          contents {
            json
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

//# run-graphql --cursors @{obj_1_3,1}
{
  queryCoinsAtChkpt1BeforeSnapshotCatchup: coins(type: "@{P0}::fake::FAKE", before: "@{cursor_0}") {
    edges {
      cursor
      node {
        consistentStateForEachCoin: owner {
          ... on AddressOwner {
            owner {
              address
              coins(type: "@{P0}::fake::FAKE") {
                edges {
                  cursor
                  node {
                    contents {
                      json
                    }
                  }
                }
              }
            }
          }
        }
        contents {
          json
        }
      }
    }
  }
  queryAddressCoinsAtChkpt1BeforeSnapshotCatchup: address(address: "@{A}") {
    coins(type: "@{P0}::fake::FAKE", before: "@{cursor_0}") {
      edges {
        cursor
        node {
          contents {
            json
          }
        }
      }
    }
  }
}

//# force-object-snapshot-catchup --start-cp 0 --end-cp 4

//# create-checkpoint

//# run-graphql --cursors @{obj_1_3,1}
{
  queryCoinsAtChkpt1AfterSnapshotCatchup: coins(type: "@{P0}::fake::FAKE", before: "@{cursor_0}") {
    edges {
      cursor
      node {
        consistentStateForEachCoin: owner {
          ... on AddressOwner {
            owner {
              address
              coins(type: "@{P0}::fake::FAKE") {
                edges {
                  cursor
                  node {
                    contents {
                      json
                    }
                  }
                }
              }
            }
          }
        }
        contents {
          json
        }
      }
    }
  }
}
