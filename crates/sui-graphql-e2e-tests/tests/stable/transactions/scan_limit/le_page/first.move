// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Without a scan limit, we would expect each query to yield a response containing two results.
// However, because we have a scanLimit of 2, we'll be limited to filtering only two candidate
// transactions per page, of which one will match the filtering criteria.

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

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 1 @B --sender B

//# run Test::M1::create --args 2 @A --sender A

//# run Test::M1::create --args 3 @B --sender B

//# run Test::M1::create --args 4 @A --sender A

//# create-checkpoint

//# run Test::M1::create --args 100 @B --sender B

//# run Test::M1::create --args 101 @A --sender A

//# run Test::M1::create --args 102 @B --sender B

//# run Test::M1::create --args 103 @A --sender A

//# run Test::M1::create --args 104 @B --sender B

//# create-checkpoint

//# run-graphql
# ten transactions total
{
  transactionBlocks(first: 50 filter: {afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# startCursor 2, endCursor 3, both scan limited
{
  transactionBlocks(first: 3 scanLimit: 2 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# startCursor: 4, endCursor 5, both scan limited
{
  transactionBlocks(first: 3 scanLimit: 2 after: "@{cursor_0}" filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":5,"i":true}
# note the changes: first 3 -> 4, scanLimit 2 -> 3
# startCursor: 6, endCursor: 8 both scanLimited
# because we've exhausted all matches in the scanned window,
# we set the endCursor to the final tx scanned, rather than snapping
# to the last matched tx
{
  transactionBlocks(first: 4 scanLimit: 3 after: "@{cursor_0}" filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":8,"i":true}
# startCursor: 9, endCursor: 11 both scanLimited
{
  transactionBlocks(first: 4 scanLimit: 3 after: "@{cursor_0}" filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":10,"i":false}
# using the last element's cursor from the previous query
# will yield an empty set, fixed on the last scannable tx
{
  transactionBlocks(first: 4 scanLimit: 3 after: "@{cursor_0}" filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":11,"i":true}
# trying to paginate on the `endCursor` even though hasNextPage is false
# cursors are null, both page flags are false
{
  transactionBlocks(first: 4 scanLimit: 3 after: "@{cursor_0}" filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
