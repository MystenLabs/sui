// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
  transactionBlocks(filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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
# startCursor 21 not scan limited, endCursor 21 is scan limited
{
  transactionBlocks(last: 1 scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":21,"i":false}
# startCursor 16, endCursor 20, both isScanLimited
# This might be a bit confusing, but the `startCursor` is 16 and not 17 because
# `scanLimit` is 5 - if we set the `startCursor` to 17, then we will never yield tx 17
# when paginating the other way, since the cursors are exclusive.
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":16,"i":true}
# continuing paginating backwards with scan limit
# startCursor 11, endCursor 15, both scan limited
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":11,"i":true}
# startCursor is 7, endCursor is 10, both scan limited
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":6,"i":true}
# startCursor is 3, not scanLimited, endCursor is 5, scanLimited
# this is because we found a matching element at tx 3, but within
# the scanned window there is another tx that we need to return for
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":3,"i":false}
# Reached the end
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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
