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

//# run Test::M1::create --args 201 @B --sender A

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
# startCursor is 2 and scanLimited, endCursor is 2 and not scanLimited
# instead of setting the endCursor to the last transaction scanned,
# we set it to the last transaction in the set
# this is so we don't end up skipping any other matches in the scan range
# but beyond the scope of the `limit`
{
  transactionBlocks(first: 1 scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":2,"i":false}
# startCursor: 3, endCursor: 7, both are scan-limited
# endCursor ends at 7, not 3, because we've exhausted all the matches
# within the window of scanning range, and will overwrite the endCursor to 7.
{
  transactionBlocks(first: 1 after: "@{cursor_0}" scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":7,"i":true}
# startCursor: 8, endCursor: 12, both are scan-limited
# expect an empty set
{
  transactionBlocks(first: 1 after: "@{cursor_0}" scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":12,"i":true}
# startCursor: 13, endCursor: 17, both are scan-limited
# single element returned, coincidentally also the last scanned transaction
{
  transactionBlocks(first: 1 after: "@{cursor_0}" scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":17,"i":true}
# startCursor: 18 scanLimited, endCursor: 18 not scanLimited
# this is because we have multiple matches within the scanning range
# but due to the `first` limit, we return a subset.
# we don't want to skip over other matches, so we don't push the endCursor out
{
  transactionBlocks(first: 1 after: "@{cursor_0}" scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":18,"i":false}
# startCursor: 19, endCursor: 21, both are scan-limited
# single element returned, coincidentally also the last scanned transaction
# note that the startCursor is 19, not 18 or 21, since we can use the scan-limited behavior
{
  transactionBlocks(first: 1 after: "@{cursor_0}" scanLimit: 5 filter: {affectedAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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
