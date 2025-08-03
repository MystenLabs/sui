// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish --sender A --upgradeable
module test::simple_module {
    public fun hello(): u64 {
        42
    }
}

//# upgrade --package test --upgrade-capability 1,1 --sender A
module test::simple_module {
    public fun hello(): u64 {
        43  // Changed value
    }
}

//# create-checkpoint

//# run-graphql
{
  # Test UpgradeCommand
  upgradeTest: transaction(digest: "@{digest_2}") {
    kind {
      ... on ProgrammableTransaction {
        commands {
          nodes {
            __typename
            ... on UpgradeCommand {
              modules
              dependencies
              currentPackage
              upgradeTicket {
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