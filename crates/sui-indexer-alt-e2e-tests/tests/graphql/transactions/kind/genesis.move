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
  # Test accessing genesis transactions directly
  genesisTransaction: transactions(first: 1) {
    nodes {
      digest
      kind {
        __typename
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

//# run-graphql
{ 
  # Test pagination functionality with genesis transactions
  paginationTest: transactions(first: 1) {
    nodes {
      digest
      kind {
        __typename
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

  backwardPaginationTest: transactions(first: 1) {
    nodes {
      digest
      kind {
        __typename
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


//# run-graphql --cursors 2
{ 
  # Test cursor-based pagination - after first cursor, get 3 objects
  paginationAfterCursor: transactions(first: 1) {
    nodes {
      digest
      kind {
        __typename
        ... on GenesisTransaction {
          objects(after: "@{cursor_0}", first: 3) {
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
            edges {
              cursor
              node {
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

//# run-graphql --cursors 5
{ 
  # Test cursor-based pagination - before a specific cursor, get last 2 objects
  paginationBeforeCursor: transactions(first: 1) {
    nodes {
      digest
      kind {
        __typename
        ... on GenesisTransaction {
          objects(before: "@{cursor_0}", last: 2) {
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
            edges {
              cursor
              node {
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
