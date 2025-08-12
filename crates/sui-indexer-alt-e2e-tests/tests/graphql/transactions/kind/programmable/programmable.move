// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::simple {
  public struct Counter has key {
    id: UID,
    value: u64,
  }

  fun init(ctx: &mut TxContext) {
    transfer::share_object(Counter {
        id: object::new(ctx),
        value: 0,
    })
  }

  public fun increment(counter: &mut Counter) {
    counter.value = counter.value + 1;
  }

  public fun add(counter: &mut Counter, amount: u64) {
    counter.value = counter.value + amount;
  }
}

//# programmable --sender A --inputs object(1,0) 42 @B
//> 0: test::simple::increment(Input(0));
//> 1: test::simple::add(Input(0), Input(1));
//> 2: TransferObjects([Gas], Input(2))

//# create-checkpoint

//# run-graphql
{
  # Test basic ProgrammableTransaction structure
  programmableTransaction: transaction(digest: "@{digest_2}") {
    digest
    kind {
      __typename
      ... on ProgrammableTransaction {
        inputs(first: 5) {
          pageInfo {
            hasNextPage
            hasPreviousPage
          }
          nodes {
            __typename
            ... on Pure {
              bytes
            }
          }
        }
        commands(first: 5) {
          pageInfo {
            hasNextPage
            hasPreviousPage
          }
          nodes {
            __typename
            ... on MoveCallCommand {
              _
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # Test pagination on the same transaction - limiting results to show pagination working
  paginationTest: transaction(digest: "@{digest_2}") {
    digest
          kind {
        __typename
        ... on ProgrammableTransaction {
          inputs(first: 2) {
          pageInfo {
            hasNextPage
            hasPreviousPage
          }
          nodes {
            __typename
            ... on Pure {
              bytes
            }
          }
        }
        commands(first: 2) {
          pageInfo {
            hasNextPage
            hasPreviousPage
          }
          nodes {
            __typename
            ... on MoveCallCommand {
              _
            }
          }
        }
      }
    }
  }
} 