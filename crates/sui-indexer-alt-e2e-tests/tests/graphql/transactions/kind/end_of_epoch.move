// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --simulator

//# advance-epoch --create-random-state --create-authenticator-state --create-deny-list-state --create-bridge-state --create-bridge-committee

//# create-checkpoint

//# run-graphql
{
  # Test EndOfEpochTransaction with multiple transaction types
  endOfEpochTransaction: transactions(last: 1) {
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
              ... on RandomnessStateCreateTransaction {
                _
              }
              ... on AuthenticatorStateCreateTransaction {
                _
              }
              ... on CoinDenyListStateCreateTransaction {
                _
              }
              ... on BridgeStateCreateTransaction {
                chainIdentifier
              }
              ... on BridgeCommitteeInitTransaction {
                bridgeObjectVersion
              }
            }
          }
        }
      }
    }
  }
}
