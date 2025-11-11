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
{
  transactionsA: transactionsScan(filter: { affectedAddress: "@{A}"}) { ...TX }
  transactionsB: transactionsScan(filter: { affectedAddress: "@{B}"}) { ...TX }
  transactionsC: transactionsScan(filter: { affectedAddress: "@{C}"}) { ...TX }
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
        }
      }
    }
  }
}

//# run-graphql
# Test multi-filter queries (affectedAddress + affectedObject)
{
  transactionsAWithObj: transactionsScan(filter: {
    affectedAddress: "@{A}",
    affectedObject: "@{obj_4_0}"
  }) { ...TX }
  transactionsBSentToA: transactionsScan(filter: {
    affectedAddress: "@{A}",
    sentAddress: "@{B}"
  }) { ...TX }
  transactionsCSentToA: transactionsScan(filter: {
    affectedAddress: "@{A}",
    sentAddress: "@{C}"
  }) { ...TX }
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
        }
      }
    }
  }
}

//# run-graphql
# Test queries with function filter (package ID)
{
  transactionsWithPackage: transactionsScan(filter: {
    function: "@{Test}"
  }) { ...TX }
  transactionsAWithPackage: transactionsScan(filter: {
    affectedAddress: "@{A}",
    function: "@{Test}"
  }) { ...TX }
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
        }
      }
    }
  }
}

//# run-graphql
# Test queries that should return empty results
{
  emptyNonExistent: transactionsScan(filter: {
    affectedAddress: "0x0000000000000000000000000000000000000000000000000000000000000001"
  }) { ...TX }
  emptyConflicting: transactionsScan(filter: {
    sentAddress: "@{A}",
    affectedAddress: "@{C}",
    beforeCheckpoint: 5
  }) { ...TX }
  emptyBeyondData: transactionsScan(filter: {
    affectedAddress: "@{A}",
    afterCheckpoint: 50000
  }) { ...TX }
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
        }
      }
    }
  }
}

//# run-graphql --cursors {"t":0,"c":0} {"t":1,"c":1} {"t":0,"c":4} {"t":2,"c":5} {"t":0,"c":10508}
{ 
  first2: transactionsScan(first: 2, filter: { affectedAddress: "@{A}" }) { ...TX }
  last2: transactionsScan(last: 2, filter: { affectedAddress: "@{A}" }) { ...TX }

  afterCp0: transactionsScan(first: 10, after: "@{cursor_0}", filter: { affectedAddress: "@{A}" }) { ...TX }
  afterCp1: transactionsScan(first: 10, after: "@{cursor_1}", filter: { affectedAddress: "@{A}" }) { ...TX }

  afterCp5Tx2: transactionsScan(first: 10, after: "@{cursor_3}", filter: { affectedAddress: "@{A}" }) { ...TX }

  beforeCp4: transactionsScan(last: 10, before: "@{cursor_2}", filter: { affectedAddress: "@{A}" }) { ...TX }

  betweenCp1AndCp4First: transactionsScan(first: 10, after: "@{cursor_1}", before: "@{cursor_2}", filter: { affectedAddress: "@{A}" }) { ...TX }
  betweenCp1AndCp4Last: transactionsScan(last: 10, after: "@{cursor_1}", before: "@{cursor_2}", filter: { affectedAddress: "@{A}" }) { ...TX }

  betweenFirstAndLast: transactionsScan(first: 50, after: "@{cursor_0}", before: "@{cursor_4}", filter: { affectedAddress: "@{A}" }) { ...TX }

  invalidOrder: transactionsScan(first: 10, after: "@{cursor_2}", before: "@{cursor_1}", filter: { affectedAddress: "@{A}" }) { ...TX }
}

fragment TX on TransactionConnection {
  pageInfo { startCursor endCursor hasPreviousPage hasNextPage }
  edges { cursor node { digest effects { checkpoint { sequenceNumber } } } }
}

//# run-graphql
{
  scanAcrossBlocks: transactionsScan(filter: {
    affectedAddress: "@{A}",
    afterCheckpoint: 0,
    beforeCheckpoint: 10510
  }) { ...TX }
}

fragment TX on TransactionConnection {
  pageInfo { startCursor endCursor hasPreviousPage hasNextPage }
  edges { cursor node { digest effects { checkpoint { sequenceNumber } } } }
}
