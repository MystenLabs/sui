// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Mirrors scan_limit/equal/first.move, paginating backwards where first and scanLimit are equal.

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

    public fun swap_value_and_send(mut lhs: Object, mut rhs: Object, recipient: address) {
        let tmp = lhs.value;
        lhs.value = rhs.value;
        rhs.value = tmp;
        transfer::public_transfer(lhs, recipient);
        transfer::public_transfer(rhs, recipient);
    }
}

//# create-checkpoint

//# run Test::M1::create --args 0 @B --sender A

//# run Test::M1::create --args 1 @B --sender A

//# run Test::M1::create --args 2 @A --sender A

//# run Test::M1::create --args 3 @A --sender A

//# run Test::M1::create --args 4 @A --sender A

//# create-checkpoint

//# run Test::M1::create --args 100 @B --sender A

//# run Test::M1::create --args 101 @A --sender A

//# run Test::M1::create --args 102 @B --sender A

//# run Test::M1::create --args 103 @A --sender A

//# run Test::M1::create --args 104 @B --sender A

//# create-checkpoint

//# run-graphql
# Expect ten results
{
  transactionBlocks(last: 50 filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
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
# boundary cursors are scan limited
# startCursor: 10, endCursor: 11
# result is single element with cursor: 11
{
  transactionBlocks(last: 2 scanLimit: 2 filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
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

//# run-graphql --cursors {"c":4,"t":10,"i":true}
# boundary cursors are scan limited
# startCursor: 9, endCursor: 9
# result is single element with cursor: 9
{
  transactionBlocks(last: 1 scanLimit: 1 before: "@{cursor_0}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
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

//# run-graphql --cursors {"c":4,"t":9,"i":true}
# boundary cursors are scan limited
# startCursor: 6, endCursor: 8
# result is single element with cursor: 7
{
  transactionBlocks(last: 3 scanLimit: 3 before: "@{cursor_0}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
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
# boundary cursors are scan limited
# startCursor: 4, endCursor: 5
# expect empty set
{
  transactionBlocks(last: 2 scanLimit: 2 before: "@{cursor_0}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
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

//# run-graphql --cursors {"c":4,"t":4,"i":true}
# Returns the first two matching transactions, boundary cursors both have `is_scan_limited: true`
# startCursor: 2, endCursor: 3
{
  transactionBlocks(last: 2 scanLimit: 2 before: "@{cursor_0}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
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


//# run-graphql --cursors {"c":4,"t":2,"i":true}
# Since we know from the previous query that there is not a previous page at this cursor,
# Expect false for page flags and null for cursors
{
  transactionBlocks(last: 2 scanLimit: 2 before: "@{cursor_0}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
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
