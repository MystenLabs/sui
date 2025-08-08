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
{ # Fetch the latest known checkpoint and version of the object
  ...State

  # Same query at the genesis checkpoint (object should not exist)
  genesis: checkpoint(sequenceNumber: 0) { query { ...State } }

  # ...again after the object was created
  created: checkpoint(sequenceNumber: 1) { query { ...State } }

  # ...again after the object was modified multiple times
  modified: checkpoint(sequenceNumber: 2) { query { ...State } }

  # ...finally after the object was left untouched.
  untouched: checkpoint(sequenceNumber: 3) { query { ...State } }

  # This checkpoint doesn't exist, so it shouldn't be possible to time-travel
  # to it
  nonexistent: checkpoint(sequenceNumber: 10) { query { ...State } }
}

fragment State on Query {
  checkpoint { sequenceNumber }
  object(address: "@{obj_1_0}") { version }
}

//# run-graphql
{ # Querying at a checkpoint hides objects that exist, but at a future
  # checkpoint.
  checkpoint(sequenceNumber: 1) {
    query {
      # Latest as of checkpoint 1
      latest: object(address: "@{obj_1_0}") { version }

      # This version does not exist, so should not return anything
      byVersion: object(address: "@{obj_1_0}", version: 4) { version }
    }
  }
}

//# run-graphql
{ # "atCheckpoint" will override the fact that this field is nested inside
  # a `Checkpoint.query`, but it still can't travel to the future relative to
  # the current latest checkpoint (which is checkpoint 1).
  checkpoint(sequenceNumber: 1) {
    query {
      atCheckpoint: object(address: "@{obj_1_0}", atCheckpoint: 4) { version }
    }
  }

}

//# run-graphql
{ # Fetch checkpoints without filters
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

//# run-graphql
{
  # Fetch checkpoints at an epoch filter, should have next page
  checkpoints(first: 2, filter: {atEpoch: 1}) {
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

//# run-graphql
{
  # Fetch checkpoints at an epoch filter, and after checkpoint 1
  checkpoints(first: 5, filter: {atEpoch: 0, afterCheckpoint: 1}) {
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

//# run-graphql
{
  # Fetch checkpoints at an epoch filter, and before checkpoint 1
  checkpoints(first: 5, filter: {atEpoch: 0, beforeCheckpoint: 2}) {
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

//# run-graphql
{
  # Fetch checkpoints at an epoch before a checkpoint not in the epoch
  checkpoints(first: 5, filter: {atEpoch: 1, beforeCheckpoint: 1}) {
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

//# run-graphql
{
  # Fetch checkpoints at nonexistent epoch
  checkpoints(first: 10, filter: {atEpoch: 5}) {
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

//# run-graphql
{ # Test all filters together (at_epoch + after_checkpoint + before_checkpoint).
  checkpoints(
    first: 10, 
    filter: {
      atEpoch: 1, 
      afterCheckpoint: 2, 
      beforeCheckpoint: 5
    }
  ) {
    edges { node { sequenceNumber } }
  }
}

//# run-graphql
{ # Test at_checkpoint filter on a checkpoint not in the epoch filter (should override other filters).
  checkpoints(
    first: 10,
    filter: {
      atEpoch: 1,
      afterCheckpoint: 2,
      beforeCheckpoint: 5,
      atCheckpoint: 3
    }
  ) {
    edges { node { sequenceNumber } }
  }
}