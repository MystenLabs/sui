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
# Because the pageLimit is one, once we find the first matching transaction,
# we return. Hence, `startCursor` = `endCursor`, and the cursors should not
# indicate that they're derived from scan limit.
{
  transactionBlocks(last: 1 scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":21,"i":false}
# Here, the `startCursor` should be `t:16` and indicate from scan limit,
# and the `endCursor` should be `t:17`, pointing to the only transaction in the result set.
# This might be a bit confusing, but the `startCursor` is 16 and not 17 because
# `scanLimit` is 5 - if we set the `startCursor` to 17, then we will never yield tx 17
# when paginating the other way, since the cursors are exclusive.
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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
# Demonstrating what happens if we use the erroneous cursor starting at 17
# This will bring us to tx 21, meaning we skip 17.
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

//# run-graphql --cursors {"c":7,"t":16,"i":true}
# Demonstrating what happens if we use the returned cursor at 16 and indicating from scan limit
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

//# run-graphql --cursors {"c":7,"t":16,"i":true}
# continue paginating backwards. Since this is an empty result, the `startCursor` is at t:11,
# and the `endCursor` is at t:15. Both should indicate they are scanLimited.
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":11,"i":true}
# Like the previous run, we get an empty set. Thus, `startCursor` is at 6, and
# `endCursor` is at 10. Both should indicate they come from scan limit.
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":6,"i":true}
# Like the previous run, we get an empty set. Thus, `startCursor` is at 6, and
# `endCursor` is at 10. Both should indicate they come from scan limit.
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":6,"i":true}
# Note that similar to earlier, the `endCursor` is at 5 and indicates is from scan limit.
# Meanwhile, the starting cursor is going to be the same as the tip of transactions.
# In the first instance, there was one more scanned beyond the matching transaction.
# In this case, the matching transaction occurs at the end.
# In both cases, we return the cursors pointing to the scanned transaction range, not
# necessarily the tips of the transactions returned.
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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
# Reached the end
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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
