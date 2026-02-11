// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses Test=0x0 --accounts A B C --simulator

//# publish
module Test::M1 {
    use sui::coin::Coin;

    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    fun foo<T: key, T2: drop>(_p1: u64, value1: T, _value2: &Coin<T2>, _p2: u64): T {
        value1
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }
}

// Transaction 1: A creates object for A (checkpoint 1)
//# run Test::M1::create --sender A --args 0 @A

//# create-checkpoint

// Transaction 2: A creates object for B (checkpoint 2) - matches A (sender) + B (recipient)
//# run Test::M1::create --sender A --args 1 @B

//# create-checkpoint

// Transaction 3: B creates object for A (checkpoint 3) - matches B (sender) + A (recipient)
//# run Test::M1::create --sender B --args 2 @A

//# create-checkpoint

// Transaction 4: C creates object for A (checkpoint 4) - matches C (sender) + A (recipient)
//# run Test::M1::create --sender C --args 3 @A

//# create-checkpoint

// Transactions 5-7: Multiple transactions in checkpoint 5 to test cursor with t > 1
//# run Test::M1::create --sender A --args 10 @A

//# run Test::M1::create --sender A --args 11 @A

//# run Test::M1::create --sender A --args 12 @A

//# create-checkpoint

//# advance-epoch

// Generate 10,501 checkpoints to create 10 complete blocks + 500 extra for incomplete block
//# create-checkpoint 10501

//# run Test::M1::create --sender A --args 100 @C

//# create-checkpoint

//# run-graphql
# Scan vs indexed cross-check, multi-filter combinations, empty-result edge cases,
# no-filter scan, and checkpoint-bounded scan across bloom blocks.
{
  scanTransactionsA: scanTransactions(filter: { affectedAddress: "@{A}" }) { ...TX }
  paginateTransactionsA: transactions(filter: { affectedAddress: "@{A}" }) { ...TX }

  transactionsAWithObj: scanTransactions(filter: {
    affectedAddress: "@{A}",
    affectedObject: "@{obj_4_0}"
  }) { ...TX }
  transactionsBSentToA: scanTransactions(filter: {
    affectedAddress: "@{A}",
    sentAddress: "@{B}"
  }) { ...TX }
  scanFnAndObj: scanTransactions(filter: {
    function: "@{Test}",
    affectedObject: "@{obj_4_0}"
  }) { ...TX }
  scanFnAndAddr: scanTransactions(filter: {
    function: "@{Test}",
    affectedAddress: "@{C}"
  }) { ...TX }
  transactionsWithPackage: scanTransactions(filter: {
    function: "@{Test}"
  }) { ...TX }

  emptyNonExistent: scanTransactions(filter: {
    affectedAddress: "0x0000000000000000000000000000000000000000000000000000000000000001"
  }) { ...TX }
  emptyConflicting: scanTransactions(filter: {
    sentAddress: "@{A}",
    affectedAddress: "@{C}",
    beforeCheckpoint: 5
  }) { ...TX }
  emptyBeyondData: scanTransactions(filter: {
    affectedAddress: "@{A}",
    afterCheckpoint: 50000
  }) { ...TX }

  noFilter: scanTransactions(first: 5, filter: {
    afterCheckpoint: 0,
    beforeCheckpoint: 6
  }) { ...TX }

  scanAcrossBlocks: scanTransactions(filter: {
    affectedAddress: "@{A}",
    afterCheckpoint: 0,
    beforeCheckpoint: 10510
  }) { ...TX }
}

fragment TX on TransactionConnection {
  pageInfo { startCursor endCursor hasPreviousPage hasNextPage }
  edges { cursor node { digest effects { checkpoint { sequenceNumber } } } }
}

//# run-graphql --cursors 5 7 20
# Cursor pagination: cursor_0 is a checkpoint boundary (cp4,tx_seq=5),
# cursor_1 is mid-checkpoint (cp5,tx_seq=7). Tests all first/last/after/before combos.
{
  first2: scanTransactions(first: 2, filter: { affectedAddress: "@{A}" }) { ...TX }
  last2: scanTransactions(last: 2, filter: { affectedAddress: "@{A}" }) { ...TX }

  firstAfter: scanTransactions(first: 10, after: "@{cursor_0}", filter: { affectedAddress: "@{A}" }) { ...TX }
  lastBefore: scanTransactions(last: 10, before: "@{cursor_1}", filter: { affectedAddress: "@{A}" }) { ...TX }
  firstBefore: scanTransactions(first: 10, before: "@{cursor_1}", filter: { affectedAddress: "@{A}" }) { ...TX }
  lastAfter: scanTransactions(last: 10, after: "@{cursor_0}", filter: { affectedAddress: "@{A}" }) { ...TX }

  windowFirst: scanTransactions(first: 10, after: "@{cursor_0}", before: "@{cursor_1}", filter: { affectedAddress: "@{A}" }) { ...TX }
  windowLast: scanTransactions(last: 10, after: "@{cursor_0}", before: "@{cursor_1}", filter: { affectedAddress: "@{A}" }) { ...TX }

  nonexistentCursor: scanTransactions(last: 10, after: "@{cursor_2}", filter: { affectedAddress: "@{A}" }) { ...TX }
  invalidOrder: scanTransactions(first: 10, after: "@{cursor_1}", before: "@{cursor_0}", filter: { affectedAddress: "@{A}" }) { ...TX }
}

fragment TX on TransactionConnection {
  pageInfo { startCursor endCursor hasPreviousPage hasNextPage }
  edges { cursor node { digest effects { checkpoint { sequenceNumber } } } }
}
