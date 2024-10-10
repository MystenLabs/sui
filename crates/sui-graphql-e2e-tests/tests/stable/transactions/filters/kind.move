// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B C D E --simulator

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
}

//# create-checkpoint

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 1 @A --sender B

//# run Test::M1::create --args 2 @A --sender C

//# run Test::M1::create --args 3 @A --sender D

//# run Test::M1::create --args 4 @A --sender E

//# create-checkpoint

//# run-graphql
{
  transactionBlocks(first: 50 filter: {kind: PROGRAMMABLE_TX atCheckpoint: 2}) {
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
{
  transactionBlocks(first: 50 filter: {kind: PROGRAMMABLE_TX atCheckpoint: 2 sentAddress: "@{A}"}) {
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
{
  transactionBlocks(first: 50 filter: {kind: PROGRAMMABLE_TX atCheckpoint: 2 sentAddress: "@{B}"}) {
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
{
  transactionBlocks(first: 50 filter: {kind: PROGRAMMABLE_TX atCheckpoint: 2 sentAddress: "@{C}"}) {
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
{
  transactionBlocks(first: 50 filter: {kind: PROGRAMMABLE_TX atCheckpoint: 2 sentAddress: "@{D}"}) {
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
{
  transactionBlocks(first: 50 filter: {kind: PROGRAMMABLE_TX atCheckpoint: 2 sentAddress: "@{E}"}) {
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
{
  transactionBlocks(first: 50 filter: {kind: SYSTEM_TX atCheckpoint: 2}) {
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
