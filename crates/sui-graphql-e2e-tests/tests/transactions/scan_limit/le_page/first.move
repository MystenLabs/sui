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
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}


//# run-graphql
# Even though we specify `first: 3`, `scanLimit: 2` means we expect to get only two results.
# Fetches the 1st and 3rd from the list of ten transactions.
{
  transactionBlocks(first: 3 scanLimit: 2 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":2}
# The query fetches the third and fifth transactions from the list of ten
{
  transactionBlocks(first: 3 scanLimit: 2 after: "@{cursor_0}" filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":6}
# The query fetches the 7th and 9th transaction from the set, also the first transaction from checkpoint 3
{
  transactionBlocks(first: 3 scanLimit: 2 after: "@{cursor_0}" filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":10}
# hasPrevPage is true, hasNextPage is false
{
  transactionBlocks(first: 3 scanLimit: 2 after: "@{cursor_0}" filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":11}
# Should yield no results, no cursors, and both pages are false
{
  transactionBlocks(first: 3 scanLimit: 2 after: "@{cursor_0}" filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":12}
# Should yield no results, no cursors, and both pages are false
{
  transactionBlocks(first: 3 scanLimit: 2 after: "@{cursor_0}" filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      endCursor
      startCursor
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}
