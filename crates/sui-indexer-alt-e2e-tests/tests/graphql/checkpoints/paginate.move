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

//# run-graphql --cursors 4
{ # Fetch checkpoints with, paginate from the back
  checkpoints(last: 2, before: "@{cursor_0}") {
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
  checkpoints(first: 5, filter: {atEpoch: 1}) {
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
  # Fetch checkpoints at an epoch that appear after a cursor
  checkpoints(first: 10, after: "@{cursor_0}", filter: {atEpoch: 0}) {
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
  # Fetch checkpoints at an epoch that appear before a cursor
  checkpoints(first: 10, filter: {atEpoch: 0}, before: "@{cursor_0}") {
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
  # Fetch checkpoints at an epoch that appear between two cursors, page from the front
  checkpoints(first: 1, filter: {atEpoch: 0}, after: "@{cursor_0}", before: "@{cursor_1}") {
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
  # Fetch checkpoints at an epoch that appear between two cursors, page from the back
  checkpoints(last: 1, filter: {atEpoch: 0}, after: "@{cursor_0}", before: "@{cursor_1}") {
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
  # Fetch checkpoints at an epoch that appear before a cursor, page from the back
  checkpoints(last: 10, filter: {atEpoch: 1}, before: "@{cursor_0}") {
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

//# run-graphql --cursors 1
{ # Test that before cursor is exclusive (should not include checkpoint 1)
  checkpoints(first: 5, before: "@{cursor_0}") {
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
{ # Test before cursor at 0 (should handle saturating_sub gracefully)
  checkpoints(first: 5, before: "@{cursor_0}") {
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