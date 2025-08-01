// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

// Create a simple transfer transaction (no AuthenticatorStateUpdate)
//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

// Create an AuthenticatorStateUpdate transaction with a single JWK
//# authenticator-state-update --round 1 --jwk-iss https://accounts.google.com --authenticator-obj-initial-shared-version 111

//# create-checkpoint

//# advance-epoch

// Create an AuthenticatorStateUpdate transaction with multiple JWKs
//# authenticator-state-update --round 2 --jwk-iss https://accounts.google.com --jwk-iss https://login.microsoftonline.com

//# create-checkpoint

//# run-graphql
{
  # Test Simple transfer transaction (should NOT match AuthenticatorStateUpdate fragment)
  nonAuthenticatorTransaction: transaction(digest: "@{digest_1}") {
    digest
    kind {
      __typename
      ... on AuthenticatorStateUpdateTransaction {
        round
        newActiveJwks {
          edges {
            cursor
            node {
              iss
              kid
              kty
              e
              n
              alg
              epoch {
                epochId
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
  # Test Single JWK transaction
  singleJwkTransaction: transaction(digest: "@{digest_2}") {
    digest
    kind {
      __typename
      ... on AuthenticatorStateUpdateTransaction {
        epoch {
          epochId
        }
        round
        newActiveJwks(first: 10) {
          pageInfo {
            hasNextPage
            hasPreviousPage
          }
          edges {
            cursor
            node {
              iss
              kid
              kty
              e
              n
              alg
              epoch {
                epochId
              }
            }
          }
        }
        authenticatorObjInitialSharedVersion
      }
    }
  }
}

//# run-graphql
{
  # Test Multiple JWKs transaction
  multipleJwksTransaction: transaction(digest: "@{digest_5}") {
    digest
    kind {
      __typename
      ... on AuthenticatorStateUpdateTransaction {
        epoch {
          epochId
        }
        round
        newActiveJwks(first: 10) {
          pageInfo {
            hasNextPage
            hasPreviousPage
          }
          edges {
            cursor
            node {
              iss
              kid
              kty
              e
              n
              alg
              epoch {
                epochId
              }
            }
          }
        }
        authenticatorObjInitialSharedVersion
      }
    }
  }
}

//# run-graphql
{
  # Test Pagination
  paginationFirstJwk: transaction(digest: "@{digest_5}") {
    digest
    kind {
      __typename
      ... on AuthenticatorStateUpdateTransaction {
        round
        newActiveJwks(first: 1) {
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
          edges {
            cursor
            node {
              iss
              kid
            }
          }
        }
      }
    }
  }

  paginationLastJwk: transaction(digest: "@{digest_5}") {
    digest
    kind {
      __typename
      ... on AuthenticatorStateUpdateTransaction {
        round
        newActiveJwks(last: 1) {
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
          edges {
            cursor
            node {
              iss
              kid
            }
          }
        }
      }
    }
  }
}
