// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests paginating forwards where first and scanLimit are equal. The 1st, 3rd, 5th, and 7th through
// 10th transactions will match the filtering criteria.

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

//# run Test::M1::create --args 1 @A --sender A

//# run Test::M1::create --args 2 @B --sender A

//# run Test::M1::create --args 3 @A --sender A

//# run Test::M1::create --args 4 @B --sender A

//# create-checkpoint

//# run Test::M1::create --args 100 @A --sender A

//# run Test::M1::create --args 101 @A --sender A

//# run Test::M1::create --args 102 @A --sender A

//# run Test::M1::create --args 103 @B --sender A

//# run Test::M1::create --args 104 @B --sender A

//# create-checkpoint

//# run-graphql
# Expect 7 results
# [2, 3, 4, 5, 6, 7, 8, 9, 10, 11] <- tx_sequence_number
# [B, A, B, A, B, A, A, A, B, B]
{
  transactionBlocks(first: 50 filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# scans [B, A] -> [2, 3]
# Because `scanLimit` is specified, both the start and end cursors should have `is_scan_limited` flag to true
# startCursor is at 2, endCursor is at 3
# The cursor for the node will have `is_scan_limited` flag set to false, because we know for sure there is
# a corresponding element for the cursor in the result set.
{
  transactionBlocks(first: 2 scanLimit: 2 filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":3,"i":true}
# scans [B] -> [4]
# Still paginating with `scanLimit`, both the start and end cursors should have `is_scan_limited` flag to true
# because of the scanLimit of 4, startCursor = endCursor = 4
{
  transactionBlocks(first: 1 scanLimit: 1 after: "@{cursor_0}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# scans [A, B, A] -> [5, 6, 7]
# both the start and end cursors should have `is_scan_limited` flag to true
# startCursor at 5, the sole element has cursor at 6, endCursor at 7
# instead of wrapping around the result set, the boundary cursors are pushed out
# to the first and last transaction scanned in this query
{
  transactionBlocks(first: 3 scanLimit: 3 after: "@{cursor_0}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":7,"i":true}
# scans [A, A] -> [8, 9]
# both the start and end cursors should have `is_scan_limited` flag to true
# startCursor at 5, the sole element has cursor at 8, endCursor at 9
# instead of returninng None, we set the boundary cursors
# to the first and last transaction scanned in this query
{
  transactionBlocks(first: 2 scanLimit: 2 after: "@{cursor_0}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# scans [B, B] -> [10, 11]
# both the start and end cursors should have `is_scan_limited` flag to true
# startCursor at 10, endCursor at 11
# correctly detects we've reached the end of the upper bound
{
  transactionBlocks(first: 2 scanLimit: 2 after: "@{cursor_0}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run Test::M1::create --args 105 @A --sender A

//# create-checkpoint

//# run-graphql --cursors {"c":4,"t":11,"i":true}
# we've introduced a new final transaction that doesn't match the filter
# both the start and end cursors should have `is_scan_limited` flag to true
# startCursor = endCursor = 12, because there is only 1 more from the given cursor,
# regardless of the specified scanLimit
# correctly detects we've reached the end of the upper bound
{
  transactionBlocks(first: 2 scanLimit: 2 after: "@{cursor_0}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 5}) {
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

//# run-graphql --cursors {"c":4,"t":12,"i":true}
# try paginating backwards on the last `endCursor`
# should yield startCursor at 10, endCursor at 11
# and the result set consists of txs 10 and 11
# the scanLimit is exclusive of the cursor, hence we reach tx 10 inclusively
# there is a next page, which is the 12th tx, which should yield an empty set
# per the filtering criteria
{
  transactionBlocks(last: 2 scanLimit: 2 before: "@{cursor_0}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 5}) {
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
