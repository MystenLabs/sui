// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

// Test 1: Single element MakeMoveVec
//# programmable --sender A --inputs 10u64
//> 0: MakeMoveVec<u64>([Input(0)]);

//# create-checkpoint

// Test 2: Multiple elements MakeMoveVec  
//# programmable --sender A --inputs 10u64 11u64
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);

//# create-checkpoint

// Test 3: Empty MakeMoveVec
//# programmable --sender A
//> 0: MakeMoveVec<u64>([]);

//# create-checkpoint

//# run-graphql
{
  # Test single element
  singleElement: transaction(digest: "@{digest_1}") {
    kind {
      ... on ProgrammableTransaction {
        commands(first: 10) {
          nodes {
            __typename
            ... on MakeMoveVecCommand {
              elements {
                __typename
                ... on Input { ix }
                ... on TxResult { cmd ix }
                ... on GasCoin { _ }
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
  # Test multiple elements
  multipleElements: transaction(digest: "@{digest_3}") {
    kind {
      ... on ProgrammableTransaction {
        commands(first: 10) {
          nodes {
            __typename
            ... on MakeMoveVecCommand {
              elements {
                __typename
                ... on Input { ix }
                ... on TxResult { cmd ix }
                ... on GasCoin { _ }
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
  # Test empty vector with type field
  emptyVector: transaction(digest: "@{digest_5}") {
    kind {
      ... on ProgrammableTransaction {
        commands(first: 10) {
          nodes {
            __typename
            ... on MakeMoveVecCommand {
              type {
                repr
                signature
              }
              elements {
                __typename
                ... on Input { ix }
                ... on TxResult { cmd ix }
                ... on GasCoin { _ }
              }
            }
          }
        }
      }
    }
  }
}
