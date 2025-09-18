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
}

//# create-checkpoint

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 1 @B --sender A

//# create-checkpoint

//# run-graphql
{
  programmable: transactions(filter: { kind: PROGRAMMABLE_TX }) { ...TX }
  programmableA: transactions(filter: { kind: PROGRAMMABLE_TX, sentAddress: "@{A}" }) { ...TX }
  programmableB_empty: transactions(filter: { kind: PROGRAMMABLE_TX, sentAddress: "@{B}" }) { ...TX }
  system: transactions(filter: { kind: SYSTEM_TX }) { ... TX }
  systemA_empty: transactions(filter: { kind: SYSTEM_TX, sentAddress: "@{A}" }) { ...TX }
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
