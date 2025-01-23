// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// epoch | checkpoints
// ------+-------------
// 0     | 4
// 1     | 4
// 2     | 2
// An additional checkpoint is created at the end.

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator

//# create-checkpoint

//# create-checkpoint

//# advance-epoch

//# run-graphql
# Even though the epoch has advanced, we will not see it until indexer indexes the next checkpoint
# in the new epoch.
{
  checkpoint {
    sequenceNumber
  }
  epoch {
    epochId
    checkpoints {
      nodes {
        sequenceNumber
      }
    }
  }
}

//# create-checkpoint

//# create-checkpoint

//# create-checkpoint

//# advance-epoch

//# create-checkpoint

//# create-checkpoint

//# run-graphql
# Get latest state
{
  checkpoint {
    sequenceNumber
  }
  epoch_0: epoch(id: 0) {
    epochId
    checkpoints {
      edges {
        cursor
        node {
          sequenceNumber
        }
      }
    }
  }
  epoch_1: epoch(id: 1) {
    epochId
    checkpoints {
      edges {
        cursor
        node {
          sequenceNumber
        }
      }
    }
  }
  epoch_2: epoch(id: 2) {
    epochId
    checkpoints {
      edges {
        cursor
        node {
          sequenceNumber
        }
      }
    }
  }
}

//# create-checkpoint

//# run-graphql --cursors {"s":3,"c":4} {"s":7,"c":8} {"s":9,"c":10}
# View checkpoints before the last checkpoint in each epoch, from the perspective of the first
# checkpoint in the next epoch.
{
  checkpoint {
    sequenceNumber
  }
  epoch_0: epoch(id: 0) {
    epochId
    checkpoints(before: "@{cursor_0}") {
      nodes {
        sequenceNumber
      }
    }
  }
  epoch_1: epoch(id: 1) {
    epochId
    checkpoints(before: "@{cursor_1}") {
      nodes {
        sequenceNumber
      }
    }
  }
  epoch_2: epoch(id: 2) {
    epochId
    checkpoints(before: "@{cursor_2}") {
      nodes {
        sequenceNumber
      }
    }
  }
}

//# run-graphql --cursors {"s":0,"c":3} {"s":4,"c":7} {"s":8,"c":9}
# View checkpoints after the first checkpoint in each epoch, from the perspective of the last
# checkpoint in each epoch.
{
  checkpoint {
    sequenceNumber
  }
  epoch_0: epoch(id: 0) {
    epochId
    checkpoints(after: "@{cursor_0}") {
      nodes {
        sequenceNumber
      }
    }
  }
  epoch_1: epoch(id: 1) {
    epochId
    checkpoints(after: "@{cursor_1}") {
      nodes {
        sequenceNumber
      }
    }
  }
  epoch_2: epoch(id: 2) {
    epochId
    checkpoints(after: "@{cursor_2}") {
      nodes {
        sequenceNumber
      }
    }
  }
}

//# run-graphql --cursors {"s":1,"c":2} {"s":5,"c":6} {"s":9,"c":9}
# View checkpoints after the second checkpoint in each epoch, from the perspective of a checkpoint
# around the middle of each epoch.
{
  checkpoint {
    sequenceNumber
  }
  epoch_0: epoch(id: 0) {
    epochId
    checkpoints(after: "@{cursor_0}") {
      nodes {
        sequenceNumber
      }
    }
  }
  epoch_1: epoch(id: 1) {
    epochId
    checkpoints(after: "@{cursor_1}") {
      nodes {
        sequenceNumber
      }
    }
  }
  epoch_2: epoch(id: 2) {
    epochId
    checkpoints(after: "@{cursor_2}") {
      nodes {
        sequenceNumber
      }
    }
  }
}
