// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --accounts A --simulator

// Limiting transactions by the checkpoint they are in

//# advance-clock --duration-ns 1

//# programmable --sender A --inputs 1 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql
{   # Top-level query, with a filter
    c0: transactionBlocks(filter: { atCheckpoint: 0 }) { nodes { ...Tx } }
    c1: transactionBlocks(filter: { atCheckpoint: 1 }) { nodes { ...Tx } }
    c2: transactionBlocks(filter: { atCheckpoint: 2 }) { nodes { ...Tx } }
    c3: transactionBlocks(filter: { atCheckpoint: 3 }) { nodes { ...Tx } }
    c4: transactionBlocks(filter: { atCheckpoint: 4 }) { nodes { ...Tx } }
}

fragment Tx on TransactionBlock {
  digest
  kind { __typename }
}

//# run-graphql
{   # Via a checkpoint query
    c0: checkpoint(id: { sequenceNumber: 0 }) { transactionBlocks { nodes { ...Tx } } }
    c1: checkpoint(id: { sequenceNumber: 1 }) { transactionBlocks { nodes { ...Tx } } }
    c2: checkpoint(id: { sequenceNumber: 2 }) { transactionBlocks { nodes { ...Tx } } }
    c3: checkpoint(id: { sequenceNumber: 3 }) { transactionBlocks { nodes { ...Tx } } }
    c4: checkpoint(id: { sequenceNumber: 4 }) { transactionBlocks { nodes { ...Tx } } }
}

fragment Tx on TransactionBlock {
  digest
  kind { __typename }
}

//# run-graphql
{   # Via paginating checkpoints
    checkpoints(first: 5) {
        pageInfo { hasNextPage }
        nodes { transactionBlocks { nodes { ...Tx } } }
    }
}

fragment Tx on TransactionBlock {
  digest
  kind { __typename }
}
