// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# programmable --sender A --inputs 42 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  # Test that kind field returns null for non-genesis transactions (programmable transactions)
  nonGenesisTransaction: transaction(digest: "@{digest_1}") {
    digest
    kind {
      ... on GenesisTransaction {
        objects {
          nodes {
            address
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # Test accessing genesis transactions through checkpoint 0
  genesisTransaction: checkpoint(sequenceNumber: 0) {
    sequenceNumber
    transactions {
      nodes {
        digest
        kind {
          ... on GenesisTransaction {
            objects(first: 50) {
              nodes {
                address
                version
              }
            }
          }
        }
      }
    }
  }
} 

//# run-graphql
{ 
  # Test pagination functionality with genesis transactions through checkpoint 0
  paginationTest: checkpoint(sequenceNumber: 0) {
    sequenceNumber
    transactions {
      nodes {
        digest
        kind {
          ... on GenesisTransaction {
            objects(first: 3) {
              nodes {
                address
                version
              }
            }
          }
        }
      }
    }
  }

  backwardPaginationTest: checkpoint(sequenceNumber: 0) {
    sequenceNumber
    transactions {
      nodes {
        digest
        kind {
          ... on GenesisTransaction {
            objects(last: 3) {
              nodes {
                address
                version
              }
            }
          }
        }
      }
    }
  }
}
