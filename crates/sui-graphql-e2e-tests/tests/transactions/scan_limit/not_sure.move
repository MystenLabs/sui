// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 48 --addresses Test=0x0 --accounts A B --simulator

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

    public fun swap_value_and_send(mut lhs: Object, mut rhs: Object, recipient: address) {
        let tmp = lhs.value;
        lhs.value = rhs.value;
        rhs.value = tmp;
        transfer::public_transfer(lhs, recipient);
        transfer::public_transfer(rhs, recipient);
    }
}

//# create-checkpoint

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 1 @A --sender A

//# run Test::M1::create --args 2 @B --sender A

//# run Test::M1::create --args 3 @B --sender B

//# run Test::M1::create --args 4 @B --sender B

//# create-checkpoint

//# run Test::M1::create --args 100 @B --sender B

//# run Test::M1::create --args 101 @B --sender B

//# run Test::M1::create --args 102 @B --sender A

//# run Test::M1::create --args 103 @B --sender A

//# run Test::M1::create --args 104 @B --sender A

//# create-checkpoint

//# run Test::M1::create --args 100 @B --sender B

//# create-checkpoint

//# run-graphql
# Test normal pagination behavior. Initial fetch should yield `startCursor` = `endCursor`.
# `hasNextPage` is true and `hasPreviousPage` is false
{
  transactionBlocks(first: 50 scanLimit: 1 filter: {signAddress: "@{A}" afterCheckpoint: 1}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    edges {
      cursor
      node {
        digest
        effects {
          checkpoint {
            sequenceNumber
          }
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":2,"i":false}
# `startCursor` = `endCursor`, both `hasNextPage` and `hasPreviousPage` are true
{
  transactionBlocks(first: 50 scanLimit: 1 after: "@{cursor_0}" filter: {signAddress: "@{A}" afterCheckpoint: 1}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    edges {
      cursor
      node {
        digest
        effects {
          checkpoint {
            sequenceNumber
          }
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":3,"i":false}
{
  transactionBlocks(first: 50 scanLimit: 1 before: "@{cursor_0}" filter: {signAddress: "@{A}" afterCheckpoint: 1}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    edges {
      cursor
      node {
        digest
        effects {
          checkpoint {
            sequenceNumber
          }
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":4,"i":false}
# `scanLimit` exceeds the position of the `before` cursor.
# Expect the endCursor to be at t:3 and `hasNextPage` to be true.
# The `endCursor` should not indicate that it comes from `scanLimit`.
# In other words, don't overwrite the behavior of default `paginate_results`
{
  transactionBlocks(first: 50 scanLimit: 10 before: "@{cursor_0}" filter: {signAddress: "@{A}" afterCheckpoint: 1}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    edges {
      cursor
      node {
        digest
        effects {
          checkpoint {
            sequenceNumber
          }
        }
      }
    }
  }
}

//# run-graphql
# Of the first 6 transactions to scan, only the first three will match.
# The `endCursor` should be overriden to point to t:6, and `hasNextPage` should be true.
# Additionally, the `endCursor` should decode to indicate it is from `scanLimit`
{
  transactionBlocks(first: 50 scanLimit: 5 filter: {signAddress: "@{A}" afterCheckpoint: 1}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    edges {
      cursor
      node {
        digest
        effects {
          checkpoint {
            sequenceNumber
          }
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":6,"i":true}
# The `startCursor` should be influenced by the `after` cursor, and have t:7.
# At the same time, t:7 should not actually show up in the result set - the next match is at t:9.
# The `endCursor` should come from the last transaction, and it should not indicate that it came from `scanLimit`.
{
  transactionBlocks(first: 50 scanLimit: 5 after: "@{cursor_0}" filter: {signAddress: "@{A}" afterCheckpoint: 1}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    edges {
      cursor
      node {
        digest
        effects {
          checkpoint {
            sequenceNumber
          }
        }
      }
    }
  }
}


//# run-graphql --cursors {"c":4,"t":11,"i":false}
# no next page and thus no `endCursor`.
# `startCursor` is at t:12 and should be from `scanLimit`
{
  transactionBlocks(first: 50 scanLimit: 5 after: "@{cursor_0}" filter: {signAddress: "@{A}" afterCheckpoint: 1}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    edges {
      cursor
      node {
        digest
        effects {
          checkpoint {
            sequenceNumber
          }
        }
      }
    }
  }
}
