// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// chkpt 1 | chkpt 2 | chkpt 3 |
// -----------------------------
// coin1@A | coin1@A | coin1@B |
// coin2@A | coin2@A | coin2@B |
// coin3@A | coin3@A | coin3@A |

//# init --protocol-version 51 --addresses P0=0x0 --accounts A B --simulator --objects-snapshot-min-checkpoint-lag 1

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

//# run-graphql
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  coins(type: "@{P0}::fake::FAKE") {
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
      }
    }
  }
}

//# transfer-object 1,1 --sender A --recipient B

//# transfer-object 1,1 --sender B --recipient A

// The above are done so there are object changes to trigger the objects snapshot processor

//# create-checkpoint

//# run-graphql
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  coins(type: "@{P0}::fake::FAKE") {
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
      }
    }
  }
}

//# transfer-object 1,2 --sender A --recipient B

//# transfer-object 1,3 --sender A --recipient B

//# create-checkpoint

//# run-graphql
# First coin owner should be different from the last two,
# and last two coin owners should be the same
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  coins(type: "@{P0}::fake::FAKE") {
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
      }
    }
  }
}

//# run-graphql --cursors bcs(@{obj_1_1},2)
# There should be two coins, and the coin owners should be the same as the owner of the first coin in the previous query
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  coins(after: "@{cursor_0}" type: "@{P0}::fake::FAKE") {
    nodes {
      owner {
        ... on AddressOwner {
          owner {
            address
          }
        }
      }
      address
      contents {
        json
      }
    }
  }
}

//# run-graphql --cursors bcs(@{obj_1_1},1)
# Outside of available range
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  coins(after: "@{cursor_0}" type: "@{P0}::fake::FAKE") {
    nodes {
      owner {
        ... on AddressOwner {
          owner {
            address
          }
        }
      }
      address
      contents {
        json
      }
    }
  }
}

//# run-graphql --cursors bcs(@{obj_1_1},0)
# Outside of available range
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  coins(after: "@{cursor_0}" type: "@{P0}::fake::FAKE") {
    nodes {
      owner {
        ... on AddressOwner {
          owner {
            address
          }
        }
      }
      address
      contents {
        json
      }
    }
  }
}
