// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Without a scan limit, we would expect each query to yield a response containing two results.
// However, because we have a scanLimit of 2, we'll be limited to filtering only two candidate
// transactions per page, of which one will match the filtering criteria.

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

//# run-graphql --cursors {"c":4,"t":4}
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


//# run-graphql --cursors {"c":4,"t":8}
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
