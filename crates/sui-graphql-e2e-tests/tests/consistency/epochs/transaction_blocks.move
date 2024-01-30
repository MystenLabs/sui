// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// epoch | transactions
// ------+-------------
// 0     | 4
// 1     | 4
// 2     | 2

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

//# create-checkpoint

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# advance-epoch

//# run-graphql
{
  checkpoint {
    sequenceNumber
  }
  epoch {
    epochId
    transactionBlocks {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
}

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# advance-epoch

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# run-graphql
# Get latest state
{
  checkpoint {
    sequenceNumber
  }
  epoch_0: epoch(id: 0) {
    epochId
    transactionBlocks {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
  epoch_1: epoch(id: 1) {
    epochId
    transactionBlocks {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
  epoch_2: epoch(id: 2) {
    epochId
    transactionBlocks {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
}

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# run-graphql --cursors {"t":3,"c":4} {"t":7,"c":8} {"t":9,"c":10}
# View transactions before the last transaction in each epoch, from the perspective of the first
# checkpoint in the next epoch.
{
  checkpoint {
    sequenceNumber
  }
  epoch_0: epoch(id: 0) {
    epochId
    transactionBlocks(before: "@{cursor_0}") {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
  epoch_1: epoch(id: 1) {
    epochId
    transactionBlocks(before: "@{cursor_1}") {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
  epoch_2: epoch(id: 2) {
    epochId
    transactionBlocks(before: "@{cursor_2}") {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
}

//# run-graphql --cursors {"t":0,"c":3} {"t":4,"c":7} {"t":8,"c":9}
# View transactions after the first transaction in each epoch, from the perspective of the last
# checkpoint in the next epoch.
{
  checkpoint {
    sequenceNumber
  }
  epoch_0: epoch(id: 0) {
    epochId
    transactionBlocks(after: "@{cursor_0}") {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
  epoch_1: epoch(id: 1) {
    epochId
    transactionBlocks(after: "@{cursor_1}") {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
  epoch_2: epoch(id: 2) {
    epochId
    transactionBlocks(after: "@{cursor_2}") {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
}

//# run-graphql --cursors {"t":1,"c":2} {"t":5,"c":6} {"t":9,"c":9}
# View transactions after the second transaction in each epoch, from the perspective of a checkpoint
# around the middle of each epoch.
{
  checkpoint {
    sequenceNumber
  }
  epoch_0: epoch(id: 0) {
    epochId
    transactionBlocks(after: "@{cursor_0}") {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
  epoch_1: epoch(id: 1) {
    epochId
    transactionBlocks(after: "@{cursor_1}") {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
  epoch_2: epoch(id: 2) {
    epochId
    transactionBlocks(after: "@{cursor_2}") {
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
}

//# run-graphql --cursors {"t":1,"c":2} {"t":5,"c":6} {"t":9,"c":9}
# Verify that with a cursor, we are locked into a view as if we were at the checkpoint stored in
# the cursor. Compare against `without_cursor`, which should show the latest state at the actual
# latest checkpoint.
{
  checkpoint {
    sequenceNumber
  }
  with_cursor: epoch(id: 1) {
    epochId
    transactionBlocks(after: "@{cursor_1}", filter: {signAddress: "@{A}"}) {
      edges {
        cursor
        node {
          digest
          sender {
            objects {
              edges {
                cursor
              }
            }
          }
        }
      }
    }
  }
  without_cursor: epoch(id: 1) {
    epochId
    transactionBlocks(filter: {signAddress: "@{A}"}) {
      edges {
        cursor
        node {
          digest
          sender {
            objects {
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
