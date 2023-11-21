// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test cursor connection pagination logic
// The implementation privileges `after`, `before`, `first`, and `last` in that order.
// Currently implemented only for items ordered in ascending order by `sequenceNumber`.
// first: 4, after: "6" -> checkpoints 7, 8, 9, 10
// first: 4, after: "6", before: "8" -> checkpoints 7
// first: 4, before: "6" -> checkpoints 0, 1, 2, 3
// last: 4, after: "6" -> checkpoints n-3, n-2, n-1, n
// last: 4, before: "6" -> checkpoints 2, 3, 4, 5

//# init --addresses Test=0x0 --simulator

//# create-checkpoint 20

//# run-graphql
{
  checkpointConnection(first: 4, after: "6") {
    nodes {
      sequenceNumber
    }
  }
}

//# run-graphql
{
  checkpointConnection(first: 4, after: "6", before: "8") {
    nodes {
      sequenceNumber
    }
  }
}

//# run-graphql
{
  checkpointConnection(first: 4, before: "6") {
    nodes {
      sequenceNumber
    }
  }
}

//# run-graphql
{
  checkpointConnection(last: 4, after: "6") {
    nodes {
      sequenceNumber
    }
  }
}

//# run-graphql
{
  checkpointConnection(last: 4, before: "6") {
    nodes {
      sequenceNumber
    }
  }
}
