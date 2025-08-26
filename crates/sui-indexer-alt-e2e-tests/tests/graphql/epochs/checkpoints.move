// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 1
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs object(1,0) 2
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# advance-epoch

//# programmable --sender A --inputs object(1,0) 1
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs object(1,0) 2
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 3
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# create-checkpoint

//# run-graphql
{ 
  # Fetch an epoch and its checkpoints
  epoch0checkpoints: epoch(epochId: 0) {
    totalCheckpoints
    checkpoints(first: 10) {
      pageInfo {
        startCursor
        endCursor
        hasPreviousPage
        hasNextPage
      }
      edges {
        node {
          sequenceNumber
          digest
          epoch {epochId}
        }
      }
    }
  }
}

//# run-graphql --cursors 1
{ 
  # Fetch an epoch and its checkpoints with cursors applied
  epoch0checkpointsAfterCursor1: epoch(epochId: 0) {
    totalCheckpoints
    checkpoints(first: 2, after: "@{cursor_0}") {
      pageInfo {
        startCursor
        endCursor
        hasPreviousPage
        hasNextPage
      }
      edges {
        node {
          sequenceNumber
          digest
          epoch {epochId}
        }
      }
    }
  }
}

//# run-graphql
{ 
  # Fetch an epoch and its checkpoints, try to filter on epoch 2, should fail
  epoch0checkpointsAtEpoch2IsNone: epoch(epochId: 0) {
    totalCheckpoints
    checkpoints(first: 10, filter: {atEpoch: 2}) {
      pageInfo {
        startCursor
        endCursor
        hasPreviousPage
        hasNextPage
      }
      edges {
        node {
          sequenceNumber
          digest
          epoch {epochId}
        }
      }
    }
  }
}

//# run-graphql
{ 
  # Fetch a nonexistent epoch and its checkpoints.
  nonexistentEpoch4checkpointsIsNone: epoch(epochId: 4) {
    totalCheckpoints
    checkpoints(first: 5) {
      pageInfo {
        startCursor
        endCursor
        hasPreviousPage
        hasNextPage
      }
      edges {
        node {
          sequenceNumber
          digest
          epoch {epochId}
        }
      }
    }
  }
}