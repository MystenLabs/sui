// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --simulator

//# advance-clock --duration-ns 1000000

//# create-checkpoint

//# run-graphql
{
  # Test the ConsensusCommitPrologue transaction created by advance-clock
  consensusCommitPrologueTransaction: transaction(digest: "@{digest_1}") {
    digest
    kind {
      __typename
      ... on ConsensusCommitPrologueTransaction {
        epoch {
          epochId
        }
        round
        commitTimestamp
        consensusCommitDigest
        subDagIndex
        additionalStateDigest
      }
    }
  }
} 
