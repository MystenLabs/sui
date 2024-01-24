// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --simulator

// Test cursor connection pagination logic
// The implementation privileges `after`, `before`, `first`, and `last` in that order.
// Currently implemented only for items ordered in ascending order by `sequenceNumber`.

// Summary of tests:
//
// F A L B | checkpoints
// --------+------------
// 4 6     |  7 - 10
// 4 6   8 |  7 -  7
// 4     6 |  0 -  3
// 4 3   6 |  4 -  5
// 4     3 |  0 -  2
//   6 4   |  9 - 12
//     4 6 |  2 -  5
//   3 4 6 |  4 -  5
//   9 4   | 10 - 12
//         |  0 - 12
// 4       |  0 -  3
//     4   |  9 - 12
// 4   2   |   error


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

//# run-graphql --cursors 3 6
{
  checkpoints(first: 4, after: "@{cursor_0}" before: "@{cursor_1}") {
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

//# run-graphql --cursors 3
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

//# run-graphql --cursors 9
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
