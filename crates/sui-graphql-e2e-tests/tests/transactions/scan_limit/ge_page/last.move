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


//# run-graphql --cursors {"c":7,"t":13}
{
  transactionBlocks(last: 1 scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":21}
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":17}
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":12}
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":7}
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":3}
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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

//# run-graphql --cursors {"c":7,"t":2}
{
  transactionBlocks(last: 1 before: "@{cursor_0}" scanLimit: 5 filter: {recvAddress: "@{A}" afterCheckpoint: 1 beforeCheckpoint: 6}) {
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
