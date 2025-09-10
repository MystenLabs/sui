// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses P1=0x0 --accounts A B --simulator

//# publish
module P1::M1 {
    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public entry fun f1(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }

    public entry fun f2(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }
}

module P1::M2 {
    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public entry fun f3(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }
}

//# create-checkpoint

//# run P1::M1::f1 --args 0 @A --sender A

//# run P1::M1::f2 --args 1 @A --sender A

//# run P1::M2::f3 --args 2 @A --sender A

//# run P1::M1::f1 --args 3 @B --sender B

//# create-checkpoint

//# run-graphql
{
  package_P1: transactions(filter: { function: "@{P1}" }) { ...TX }
  package_module_P1M1: transactions(filter: { function: "@{P1}::M1" }) { ...TX }
  package_module_member_P1M1f2: transactions(filter: { function: "@{P1}::M1::f2" }) { ...TX }
  package_P1_sentAddress_B: transactions(filter: { function: "@{P1}", sentAddress: "@{B}" }) { ...TX }
}

fragment TX on TransactionConnection {
  pageInfo {
    startCursor
    endCursor
    hasPreviousPage
    hasNextPage
  }
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
