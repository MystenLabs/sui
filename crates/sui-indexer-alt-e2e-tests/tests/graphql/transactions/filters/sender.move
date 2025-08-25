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

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 0 @A --sender A

//# run Test::M1::create --args 0 @A --sender A

//# create-checkpoint

//# run Test::M1::create --args 0 @A --sender B

//# run Test::M1::create --args 0 @A --sender B

//# create-checkpoint

//# run-graphql --cursors 2 3
{
  allTransactions: transactions(first: 50) {
    ...TxConnection
  }

  transactionsByA: transactions(first: 10, filter: { sender: "@{A}" }) {
    ...TxConnection
  }

  transactionsByAPaginated: transactions(first: 2, after: "@{cursor_0}", filter: { sender: "@{A}" }) {
    ...TxConnection
  }

  transactionsByAPaginatedBackward: transactions(last: 2, before: "@{cursor_1}", filter: { sender: "@{A}" }) {
    ...TxConnection
  }
}

fragment TxConnection on TransactionConnection {
  edges {
    cursor
    node {
      digest
      sender {
        address
      }
    }
  }
  pageInfo {
    hasNextPage
    hasPreviousPage
    startCursor
    endCursor
  }
}