// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// checkpoint | transactions from A
// -----------+--------------------
// 0          | 0
// 1          | 4
// 2          | 3
// 3          | 2

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
}

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# run-graphql
# Each transaction block's sender's last object resolves to the same last object at the latest
# state
{
  checkpoints {
    nodes {
      sequenceNumber
      transactionBlocks(filter: { sentAddress: "@{A}"}) {
        edges {
          cursor
          node {
            digest
            sender {
                objects(last: 1) {
                    edges {
                        cursor
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
# Similarly, resolving the checkpoint's epoch will return the epoch the checkpoint belongs in, but
# the nested checkpoints connection will behave as if it was made on the top-level.
{
  checkpoints {
    nodes {
      sequenceNumber
      epoch {
        epochId
        checkpoints {
          nodes {
            sequenceNumber
          }
        }
      }
    }
  }
}
