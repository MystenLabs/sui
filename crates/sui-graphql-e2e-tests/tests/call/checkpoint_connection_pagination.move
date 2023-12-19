// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test cursor connection pagination logic
// The implementation privileges `after`, `before`, `first`, and `last` in that order.
// Currently implemented only for items ordered in ascending order by `sequenceNumber`.

// Assuming checkpoints 0 through 12
// first: 4, after: "6" -> checkpoints 7, 8, 9, 10
// first: 4, after: "6", before: "8" -> checkpoints 7
// first: 4, before: "6" -> error
// last: 4, after: "6" -> error
// last: 4, before: "6" -> checkpoints 2, 3, 4, 5
// last: 4, before: "6", after: "3" -> error
// no first or last -> checkpoints 0, 1, 2, 3
// first: 4 -> checkpoints 0, 1, 2, 3
// last: 4 -> checkpoints 9, 10, 11, 12

//# init --addresses Test=0x0 --simulator

//# create-checkpoint 12

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

//# run-graphql
{
  checkpointConnection(last: 4, before: "6", after: "3") {
    nodes {
      sequenceNumber
    }
  }
}

//# run-graphql
{
  checkpointConnection {
    nodes {
      sequenceNumber
    }
  }
}


//# run-graphql
{
  checkpointConnection(first: 4) {
    nodes {
      sequenceNumber
    }
  }
}


//# run-graphql
{
  checkpointConnection(last: 4) {
    nodes {
      sequenceNumber
    }
  }
}
