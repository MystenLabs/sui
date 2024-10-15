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
  transactionBlocks(last: 50 filter: {afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# startCursor: 10, endCursor: 11, both scan limited
{
  transactionBlocks(last: 3 scanLimit: 2 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# startCursor: 8, endCursor: 9, both scan limited
{
  transactionBlocks(last: 3 before: "@{cursor_0}" scanLimit: 2 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":8,"i":false}
# use result's cursor instead of boundary cursor
# startCursor: 6, endCursor: 7, both scan limited
{
  transactionBlocks(last: 3 before: "@{cursor_0}" scanLimit: 2 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# startCursor: 4, endCursor: 5, both scan limited
{
  transactionBlocks(last: 3 before: "@{cursor_0}" scanLimit: 2 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":4,"i":false}
# reached the end with this query
{
  transactionBlocks(last: 3 before: "@{cursor_0}" scanLimit: 2 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# cursors are null, and page flags are both false
{
  transactionBlocks(last: 3 before: "@{cursor_0}" scanLimit: 2 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
