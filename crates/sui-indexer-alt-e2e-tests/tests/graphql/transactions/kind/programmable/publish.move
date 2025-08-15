// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish --sender A
module test::simple_module {
    public fun hello(): u64 {
        42
    }
}

//# create-checkpoint

//# run-graphql
{
  # Test PublishCommand
  publishTest: transaction(digest: "@{digest_1}") {
    kind {
      ... on ProgrammableTransaction {
        commands {
          nodes {
            __typename
            ... on PublishCommand {
              modules
              dependencies
            }
          }
        }
      }
    }
  }
} 