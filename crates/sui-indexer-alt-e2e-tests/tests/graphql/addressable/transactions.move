// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses Test=0x0 --accounts A B --simulator

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

// A sends transaction to A (affects A, sent by A)
//# run Test::M1::create --sender A --args 0 @A

//# create-checkpoint

// A sends transaction to B (affects B, sent by A)  
//# run Test::M1::create --sender A --args 1 @B

//# create-checkpoint

// B sends transaction to A (affects A, sent by B)
//# run Test::M1::create --sender B --args 2 @A

//# create-checkpoint

//# run-graphql
{
  # Test default behavior (SENT relationship)
  addressA_sent: address(address: "@{A}") {
    transactions {
      ...TX
    }
  }
  
  # Test explicit SENT relationship
  addressA_sent_explicit: address(address: "@{A}") {
    transactions(relation: SENT) {
      ...TX
    }
  }
  
  # Test AFFECTED relationship
  addressA_affected: address(address: "@{A}") {
    transactions(relation: AFFECTED) {
      ...TX
    }
  }
  
  # Test B's transactions (should have 1 sent, 1 affected)
  addressB_sent: address(address: "@{B}") {
    transactions(relation: SENT) {
      ...TX
    }
  }
  
  addressB_affected: address(address: "@{B}") {
    transactions(relation: AFFECTED) {
      ...TX
    }
  }
  
  # Test with additional filters
  addressA_sent_with_filter: address(address: "@{A}") {
    transactions(relation: SENT, filter: { sentAddress: "@{A}" }) {
      ...TX
    }
  }
  
  # Test pagination
  addressA_affected_first1: address(address: "@{A}") {
    transactions(relation: AFFECTED, first: 1) {
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
      edges {
        cursor
        node {
          digest
        }
      }
    }
  }
}

fragment TX on TransactionConnection {
  edges {
    cursor
    node {
      digest
      effects {
        checkpoint {
          sequenceNumber
          digest
        }
      }
    }
  }
}
