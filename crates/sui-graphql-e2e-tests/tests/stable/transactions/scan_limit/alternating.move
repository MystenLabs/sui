// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Testing behavior of alternating between a scan-limited and normal query

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
{
  transactionBlocks(first: 2 scanLimit: 2 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":3,"t":3,"i":true}
# This should return the next two matching transactions after 3,
# so tx 4 and 6. the boundary cursors should wrap the response set,
# and both should have isScanLimited set to false
{
  transactionBlocks(first: 2 after: "@{cursor_0}" filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":3,"t":3,"i":true}
# Meanwhile, because of the scanLimit of 2, the boundary cursors are
# startCursor: 4, endCursor: 5, and both are scan limited
{
  transactionBlocks(first: 2 after: "@{cursor_0}" scanLimit: 2 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":3,"t":6,"i":false}
# From a previous query that was not scan limited, paginate with scan limit
# startCursor: 7, endCursor: 8, both scan limited
# response set consists of single tx 8
{
  transactionBlocks(first: 2 after: "@{cursor_0}" scanLimit: 2 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":3,"t":5,"i":true}
# from tx 5, select the next two transactions that match
# setting the scanLimit to impose all of the remaining txs
# even though we've finished scanning
# we should indicate there is a next page so we don't skip any txs
# consequently, the endCursor wraps the result set
# startCursor: 6, endCursor: 8, endCursor is not scan limited
{
  transactionBlocks(first: 2 after: "@{cursor_0}" scanLimit: 6 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":3,"t":8,"i":false}
# fetch the last tx without scan limit
# startCursor = endCursor = 10, wrapping the response set
{
  transactionBlocks(first: 2 after: "@{cursor_0}" filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":3,"t":8,"i":false}
# fetch the last tx with scan limit
# unlike the not-scan-limited query, the start and end cursors
# are expanded out to the scanned window, instead of wrapping the response set
{
  transactionBlocks(first: 2 after: "@{cursor_0}" scanLimit: 6 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
