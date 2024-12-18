// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tx | func           | checkpoint | epoch
// ---+----------------+------------+-------
//  0 |                |         0 |     0
//  1 | make_immutable |         1 |     0
//  2 | create         |         2 |     0
//  3 | epoch          |         3 |     0
//  4 | create         |         4 |     1
//  5 | create         |         5 |     1
//  6 | create         |         6 |     1
//  7 | epoch          |         7 |     1
//  8 | create         |         8 |     2
//  9 | create         |         9 |     2
// 10 | create         |        10 |     2
// 11 | epoch          |        11 |     2
// 12 | epoch          |        12 |     3

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

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# advance-epoch

//# advance-epoch

//# run-graphql --cursors {"t":3,"i":false,"c":4} {"t":7,"i":false,"c":8} {"t":11,"i":false,"c":12}
# View transactions before the last transaction in each epoch, from the perspective of the first
# checkpoint in the next epoch.
{
  checkpoint {
    sequenceNumber
  }
  epoch_0_txs: epoch(id: 0) {
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
  txs_epoch_0: transactionBlocks(before: "@{cursor_0}") {
    edges {
      cursor
      node {
        digest
      }
    }
  }
  epoch_1_txs: epoch(id: 1) {
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
  txs_epoch_1: transactionBlocks(before: "@{cursor_1}") {
    edges {
      cursor
      node {
        digest
      }
    }
  }
  epoch_2_txs: epoch(id: 2) {
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
  txs_epoch_2: transactionBlocks(before: "@{cursor_2}") {
    edges {
      cursor
      node {
        digest
      }
    }
  }
}

//# run-graphql --cursors {"t":0,"i":false,"c":7} {"t":4,"i":false,"c":11} {"t":8,"i":false,"c":12}
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

//# run-graphql --cursors {"t":1,"i":false,"c":2} {"t":5,"i":false,"c":6} {"t":9,"i":false,"c":10}
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

//# run-graphql --cursors {"t":5,"i":false,"c":6}
# Verify that with a cursor, we are locked into a view as if we were at the checkpoint stored in
# the cursor. Compare against `without_cursor`, which should show the latest state at the actual
# latest checkpoint. There should only be 1 transaction block in the `with_cursor` query, but
# multiple in the second
{
  checkpoint {
    sequenceNumber
  }
  with_cursor: transactionBlocks(after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
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
  without_cursor: transactionBlocks(filter: {sentAddress: "@{A}"}) {
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
