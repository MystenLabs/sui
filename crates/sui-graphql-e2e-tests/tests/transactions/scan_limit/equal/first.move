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

//# run Test::M1::create --args 2 @A --sender A

//# run Test::M1::create --args 3 @A --sender A

//# run Test::M1::create --args 4 @A --sender A

//# create-checkpoint

//# run Test::M1::create --args 100 @A --sender A

//# run Test::M1::create --args 101 @A --sender A

//# run Test::M1::create --args 102 @A --sender A

//# run Test::M1::create --args 103 @A --sender A

//# run Test::M1::create --args 104 @A --sender A

//# create-checkpoint

//# run-graphql
# Expect ten results
{
  transactionBlocks(first: 50 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# With a scanLimit of 1, we should get a transaction whose digest corresponds to the first of the
# previous result, and `hasNextPage` should be true
{
  transactionBlocks(first: 1 scanLimit: 1 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":2,"i":false}
# The query fetches the second transaction from the list of ten
{
  transactionBlocks(first: 1 scanLimit: 1 after: "@{cursor_0}" filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":6,"i":false}
# The query fetches the sixth transaction from the set, also the first transaction from checkpoint 3
{
  transactionBlocks(first: 1 scanLimit: 1 after: "@{cursor_0}" filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":10,"i":false}
# Fetches the last transaction, hasPrevPage is true, hasNextPage is false
{
  transactionBlocks(first: 1 scanLimit: 1 after: "@{cursor_0}" filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":11,"i":false}
# Should yield no results, no cursors, and both pages are false
{
  transactionBlocks(first: 1 scanLimit: 1 after: "@{cursor_0}" filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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

//# run-graphql --cursors {"c":4,"t":12,"i":false}
# Should yield no results, no cursors, and both pages are false
{
  transactionBlocks(first: 1 scanLimit: 1 after: "@{cursor_0}" filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
