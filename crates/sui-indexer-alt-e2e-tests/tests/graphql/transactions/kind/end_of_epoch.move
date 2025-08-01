// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --simulator

//# advance-epoch

//# create-checkpoint

//# run-graphql
{
  # Test finding EndOfEpochTransaction in recent transactions
  recentTransactions: transactions(last: 1) {
    nodes {
      digest
      kind {
        __typename
        ... on EndOfEpochTransaction {
          transactions {
            nodes {
              __typename
              ... on ChangeEpochTransaction {
                epoch {
                  epochId
                }
                protocolConfigs {
                  protocolVersion
                }
                storageCharge
                computationCharge
                storageRebate
                nonRefundableStorageFee
                epochStartTimestamp
              }
            }
          }
        }
      }
    }
  }
}
