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

//# run Test::M1::create --args 0 @B --sender A

//# run Test::M1::create --args 1 @B --sender A

//# run Test::M1::create --args 2 @B --sender A

//# run Test::M1::create --args 3 @B --sender A

//# run Test::M1::create --args 4 @B --sender A

//# create-checkpoint

//# run Test::M1::create --args 100 @B --sender A

//# run Test::M1::create --args 101 @B --sender A

//# run Test::M1::create --args 102 @B --sender A

//# run Test::M1::create --args 103 @B --sender A

//# run Test::M1::create --args 104 @B --sender A

//# create-checkpoint

//# run-graphql
# Expect ten results
{
  transactionBlocks(filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# Don't need scanLimit with sender
{
  transactionBlocks(filter: {sentAddress: "@{A}" affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
# scanLimit required
{
  transactionBlocks(filter: {sentAddress: "@{A}" affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4 function: "@{Test}::M1::create"}) {
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
# valid
{
  transactionBlocks(scanLimit: 50 filter: {sentAddress: "@{A}" affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4 function: "@{Test}::M1::create"}) {
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
# scanLimit required
{
  transactionBlocks(filter: {sentAddress: "@{A}" affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4 kind: PROGRAMMABLE_TX}) {
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
# valid
{
  transactionBlocks(scanLimit: 50 filter: {sentAddress: "@{A}" affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4 kind: PROGRAMMABLE_TX}) {
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
# scanLimit required
{
  transactionBlocks(filter: {sentAddress: "@{A}" affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4 inputObject: "@{obj_3_0}"}) {
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
# valid
{
  transactionBlocks(scanLimit: 50 filter: {sentAddress: "@{A}" affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4 inputObject: "@{obj_3_0}"}) {
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
# scanLimit required
{
  transactionBlocks(filter: {sentAddress: "@{A}" affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4 changedObject: "@{obj_3_0}"}) {
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
# Only one of the transactions will match this filter
# Because scanLimit is specified, the boundary cursors should be at 2 and 11,
# and both will indicate is_scan_limited
{
  transactionBlocks(scanLimit: 50 filter: {sentAddress: "@{A}" affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4 changedObject: "@{obj_3_0}"}) {
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
