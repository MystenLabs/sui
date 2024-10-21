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

//# run-graphql --cursors {"c":4,"t":2,"i":true} {"c":4,"t":7,"i":true}
# startCursor is at 3 + scanLimited, endCursor at 4 + not scanLimited
# this is because between (2, 7), txs 4 and 6 match, and thus endCursor snaps to last of result
{
  transactionBlocks(first: 1 scanLimit: 4 after: "@{cursor_0}" before: "@{cursor_1}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":2,"i":true} {"c":4,"t":7,"i":true}
# startCursor is at 3 + scanLimited, endCursor at 6 + scanLimited
# we return txs 4 and 6, paginate_results thinks we do not have a next page,
# and scan-limit logic will override this as there are still more txs to scan
# note that we're scanning txs [3, 6]
{
  transactionBlocks(first: 3 scanLimit: 4 after: "@{cursor_0}" before: "@{cursor_1}" filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":4,"i":true} {"c":4,"t":8,"i":true}
# txs 5 and 7 match, but due to page size of `first: 1`, we only return tx 5
# startCursor is 5 + scan limited, endCursor is also 5 + scan limited
{
  transactionBlocks(first: 1 scanLimit: 3 after: "@{cursor_0}" before: "@{cursor_1}" filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
