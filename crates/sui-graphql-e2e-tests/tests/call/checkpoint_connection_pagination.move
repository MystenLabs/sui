// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test cursor connection pagination logic
// The implementation privileges `after`, `before`, `first`, and `last` in that order.
// Currently implemented only for items ordered in ascending order by `sequenceNumber`.

// Assuming checkpoints 0 through 12
// first: 4, after: 6 -> checkpoints 7, 8, 9, 10
// first: 4, after: 6, before: 8 -> checkpoints 7
// first: 4, before: 6 -> checkpoints 0, 1, 2, 3
// last: 4, after: 6 -> checkpoints 9, 10, 11, 12
// last: 4, before: 6 -> checkpoints 2, 3, 4, 5
// last: 4, after: 3, before: 6 -> checkpoints 4, 5
// no first or last -> checkpoints 0, 1, ..., 11, 12
// first: 4 -> checkpoints 0, 1, 2, 3
// last: 4 -> checkpoints 9, 10, 11, 12
// first: 4, last: 2 -> error

//# init --addresses Test=0x0 --simulator

//# create-checkpoint 12

//# run-graphql --cursors 6
{
  checkpoints(first: 4, after: "@{cursor_0}") {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node { sequenceNumber }
    }
  }
}

//# run-graphql --cursors 6 8
{
  checkpoints(first: 4, after: "@{cursor_0}", before: "@{cursor_1}") {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node { sequenceNumber }
    }
  }
}

//# run-graphql --cursors 6
{
  checkpoints(first: 4, before: "@{cursor_0}") {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node { sequenceNumber }
    }
  }
}

//# run-graphql --cursors 6
{
  checkpoints(last: 4, after: "@{cursor_0}") {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node { sequenceNumber }
    }
  }
}

//# run-graphql --cursors 6
{
  checkpoints(last: 4, before: "@{cursor_0}") {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node { sequenceNumber }
    }
  }
}

//# run-graphql --cursors 3 6
{
  checkpoints(last: 4, after: "@{cursor_0}" before: "@{cursor_1}") {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node { sequenceNumber }
    }
  }
}

//# run-graphql
{
  checkpoints {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node { sequenceNumber }
    }
  }
}

//# run-graphql
{
  checkpoints(first: 4) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node { sequenceNumber }
    }
  }
}

//# run-graphql
{
  checkpoints(last: 4) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node { sequenceNumber }
    }
  }
}

//# run-graphql
{
  checkpoints(first: 4, last: 2) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node { sequenceNumber }
    }
  }
}
