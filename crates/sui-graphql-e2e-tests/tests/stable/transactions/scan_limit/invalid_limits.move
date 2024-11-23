// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// For any instance where limit is 0 or scan limit is 0, we should return an empty result

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

//# run Test::M1::create --args 1 @A --sender A

//# run Test::M1::create --args 2 @B --sender A

//# run Test::M1::create --args 3 @A --sender A

//# run Test::M1::create --args 4 @B --sender A

//# create-checkpoint

//# run-graphql
{
  transactionBlocks(first: 0 scanLimit: 2 filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
{
  transactionBlocks(first: 2 scanLimit: 0 filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
{
  transactionBlocks(first: 0 scanLimit: 0 filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
{
  transactionBlocks(first: 0 filter: {affectedAddress: "@{B}" afterCheckpoint: 1 beforeCheckpoint: 4}) {
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
