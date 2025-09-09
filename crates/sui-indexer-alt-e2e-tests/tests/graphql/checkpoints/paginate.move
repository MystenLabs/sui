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
  # Test basic pagination without filters
  allCheckpoints: checkpoints(first: 10) {
    pageInfo { ...PageInfoFields }
    edges {
      node { ...CheckpointFields }
    }
  }
  
  # Test pagination with epoch filter
  paginatedCheckpointsAtEpochHasNextPage: checkpoints(first: 2, filter: {atEpoch: 1}) {
    pageInfo { ...PageInfoFields }
    edges {
      node { ...CheckpointFields }
    }
  }

  # Test pagination with epoch filter
  paginatedCheckpointsAtEpochHasPreviousPage: checkpoints(last: 2, filter: {atEpoch: 1}) {
    pageInfo { ...PageInfoFields }
    edges {
      node { ...CheckpointFields }
    }
  }
  
  # Test filtering for non-existent epoch
  checkpointsAtNonExistentEpoch: checkpoints(first: 10, filter: {atEpoch: 5}) {
    pageInfo { ...PageInfoFields }
    edges {
      node { ...CheckpointFields }
    }
  }
}

fragment CheckpointFields on Checkpoint {
  sequenceNumber
  digest
  epoch {epochId}
}

fragment PageInfoFields on PageInfo {
  startCursor
  endCursor
  hasPreviousPage
  hasNextPage
}

//# run-graphql --cursors 6
{
  checkpointsAtEpoch1BeforeCp6FromBackHasNextPage: checkpoints(last: 1, before: "@{cursor_0}", filter: {atEpoch: 1}) {
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

//# run-graphql --cursors 0
{
  checkpointsAfterCp0HasPreviousPreviousPage: checkpoints(first: 1, after: "@{cursor_0}") {
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

//# run-graphql --cursors 4
{
  checkpointsBeforeCp4FromBack: checkpoints(last: 2, before: "@{cursor_0}") {
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

//# run-graphql --cursors 0
{
  checkpointsAtEpoch0AfterCp0: checkpoints(first: 10, after: "@{cursor_0}", filter: {atEpoch: 0}) {
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

//# run-graphql  --cursors 3
{
  checkpointsAtEpoch0BeforeCp3: checkpoints(first: 10, filter: {atEpoch: 0}, before: "@{cursor_0}") {
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

//# run-graphql  --cursors 0 3
{
  checkpointsAtEpoch0BetweenCp03FromFront: checkpoints(first: 1, filter: {atEpoch: 0}, after: "@{cursor_0}", before: "@{cursor_1}") {
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
  checkpointsAtEpoch0BetweenCp03FromBack: checkpoints(last: 1, filter: {atEpoch: 0}, after: "@{cursor_0}", before: "@{cursor_1}") {
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

//# run-graphql  --cursors 6
{
  checkpointsAtEpoch1BeforeCp6FromBack: checkpoints(last: 10, filter: {atEpoch: 1}, before: "@{cursor_0}") {
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

//# run-graphql --cursors 1
{ 
  # Test that before cursor is exclusive (should not include checkpoint 1)
  checkpointsBeforeCp1: checkpoints(first: 5, before: "@{cursor_0}") {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    edges {
      node {
        sequenceNumber
      }
    }
  }
}

//# run-graphql --cursors 0
{ 
  # Test before cursor at 0 (should not include checkpoint 0)
  checkpointsBeforeCp0: checkpoints(first: 5, before: "@{cursor_0}") {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    edges {
      node {
        sequenceNumber
      }
    }
  }
}