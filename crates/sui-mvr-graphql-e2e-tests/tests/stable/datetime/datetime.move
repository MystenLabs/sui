// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --simulator

//# create-checkpoint

// advance the clock by 1ms, next checkpoint timestmap should be 1970-01-01T00:00:00:001Z
//# advance-clock --duration-ns 1000000

//# create-checkpoint

// advance the clock by 1ms, next checkpoint timestmap should be 1970-01-01T00:00:00:002Z
//# advance-clock --duration-ns 1000000

//# create-checkpoint

// advance the clock by 1ms, next checkpoint timestmap should be 1970-01-01T00:00:00:003Z
//# advance-clock --duration-ns 1000000

//# create-checkpoint

// advance the clock by 10ms, next checkpoint timestmap should be 1970-01-01T00:00:00:013Z
//# advance-clock --duration-ns 10000000

//# create-checkpoint

// advance the clock by 2000ms, next checkpoint timestmap should be 1970-01-01T00:00:02:013Z
//# advance-clock --duration-ns 2000000000

//# create-checkpoint

// advance the clock by 990s / 16m30s, next checkpoint timestmap should be 1970-01-01T00:16:32.013Z
//# advance-clock --duration-ns 990000000000

//# create-checkpoint

// advance the clock by 9900s / 2h45m0s, next checkpoint timestmap should be 1970-01-01T03:01:32.013Z
//# advance-clock --duration-ns 9900000000000

//# advance-epoch

//# create-checkpoint

// advance the clock by 1888ms, next checkpoint timestmap should be 1970-01-01T03:01:33:901Z
//# advance-clock --duration-ns 1888000000

// advance the clock by 99ms, next checkpoint timestmap should be 1970-01-01T03:01:34:00Z
//# advance-clock --duration-ns 99000000

//# create-checkpoint

//# advance-epoch

//# run-graphql
{
  checkpoints(last: 10) {
    nodes {
      sequenceNumber
      timestamp
      epoch {
        epochId
      }
    }
  }
}

//# run-graphql
# Query for the system transaction that corresponds to a checkpoint (note that
# its timestamp is advanced, because the clock has advanced).
{
  transactionBlocks(last: 10) {
    nodes {
      kind {
        __typename
        ... on ConsensusCommitPrologueTransaction {
          epoch {
            epochId
          }
          commitTimestamp
          consensusCommitDigest
        }
      }
    }
  }
}
