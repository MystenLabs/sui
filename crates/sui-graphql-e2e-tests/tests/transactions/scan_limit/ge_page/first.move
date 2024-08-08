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

//# run Test::M1::create --args 2 @B --sender B

//# run Test::M1::create --args 3 @B --sender B

//# run Test::M1::create --args 4 @B --sender B

//# create-checkpoint

//# run Test::M1::create --args 100 @B --sender B

//# run Test::M1::create --args 101 @B --sender B

//# run Test::M1::create --args 102 @B --sender B

//# run Test::M1::create --args 103 @B --sender B

//# run Test::M1::create --args 104 @B --sender B

//# create-checkpoint

//# run Test::M1::create --args 100 @B --sender B

//# run Test::M1::create --args 101 @B --sender B

//# run Test::M1::create --args 102 @B --sender B

//# run Test::M1::create --args 103 @B --sender B

//# run Test::M1::create --args 104 @B --sender B

//# create-checkpoint

//# run Test::M1::create --args 200 @A --sender A

//# run Test::M1::create --args 201 @B --sender B

//# run Test::M1::create --args 202 @B --sender B

//# run Test::M1::create --args 203 @B --sender B

//# run Test::M1::create --args 204 @A --sender A

//# create-checkpoint

//# run-graphql
{
  transactionBlocks(filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
    pageInfo {
      startCursor
      hasPreviousPage
      endCursor
      hasNextPage
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
{
  transactionBlocks(first: 1 scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
    pageInfo {
      startCursor
      hasPreviousPage
      endCursor
      hasNextPage
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

//# run-graphql --cursors {"c":7,"t":2,"i":false}
# the `endCursor` should be at t:7 and should indicate it is a scan-limited cursor.
# this is because we've returned less than the expected page-size number of results.
# instead of setting the `endCursor` to the final element of the results,
# we can return the last transaction scanned.
{
  transactionBlocks(first: 1 after: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
    pageInfo {
      startCursor
      hasPreviousPage
      endCursor
      hasNextPage
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


//# run-graphql --cursors {"c":7,"t":3,"i":false}
# We should not receive this cursor when paginating,
# but if someone does craft it, we'd expect the result set to be empty.
# the `endCursor` would now be at t:8, and indicate that it cane from a scan limit.
{
  transactionBlocks(first: 1 after: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
    pageInfo {
      startCursor
      hasPreviousPage
      endCursor
      hasNextPage
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

//# run-graphql --cursors {"c":7,"t":7,"i":true}
# This is another page that will yield an empty result.
# consequently, the `startCursor` will be at t:8, which is the starting cursor + 1,
# and it should indicate that it came from a scan limit. Similarly, the `endCursor`
# would be at t:12, and indicate that it came from a scan limit as well.
{
  transactionBlocks(first: 1 after: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
    pageInfo {
      startCursor
      hasPreviousPage
      endCursor
      hasNextPage
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


//# run-graphql --cursors {"c":7,"t":12,"i":true}
# Starting but not including t:12, we will scan transactions 13 through 17.
# Because we have `first` number of transactions, we can use the result set
# directly to determine the `endCursor`. Coincidentally, this is the same as
# the scan-limited cursor.
# Because the given `after` cursor was a scan limited cursor, this page's
# startCursor is at t:13 and indicates that it also came from a scan limited cursor.
{
  transactionBlocks(first: 1 after: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
    pageInfo {
      startCursor
      hasPreviousPage
      endCursor
      hasNextPage
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

//# run-graphql --cursors {"c":7,"t":17,"i":false}
# The very last transaction in the tx range and in the scanning range
# matches the criteria. `startCursor = endCursor = t:21` and should not
# be a scan-limited cursor.
{
  transactionBlocks(first: 1 after: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
    pageInfo {
      startCursor
      hasPreviousPage
      endCursor
      hasNextPage
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
