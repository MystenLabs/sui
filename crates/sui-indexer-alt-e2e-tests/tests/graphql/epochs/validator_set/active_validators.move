// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# run-graphql
{
  epoch(epochId: 0) {
    epochId
    validatorSet {
      activeValidators {
        pageInfo {
         hasPreviousPage
         hasNextPage
         startCursor
         endCursor
        }
        nodes {
          address
          balance(coinType: "0x2::sui::SUI") {
            totalBalance
          }
          balances {
            __typename
          }
          # todo DVX-1697 populate defaultSuinsName
          defaultSuinsName
          multiGetBalances(keys: ["0x2::sui::SUI"]) {
            totalBalance
          }
          objects {
            __typename
          }
          credentials { ...VC }
          # todo DVX-1697 populate nextEpochCredentials
          nextEpochCredentials { ...VC }
          name
          # todo DVX-1697 populate description
          description
          # todo DVX-1697 populate imageUrl
          imageUrl
          # todo DVX-1697 populate projectUrl
          projectUrl
          operationCap {
            address
          }
          stakingPoolId
          stakingPoolActivationEpoch
          stakingPoolSuiBalance
          # todo DVX-1697 populate rewardsPool
          rewardsPool
          poolTokenBalance
          # todo DVX-1697 populate pendingStake
          pendingStake
          # todo DVX-1697 populate pendingTotalSuiWithdraw
          pendingTotalSuiWithdraw
          # todo DVX-1697 populate pendingPoolTokenWithdraw
          pendingPoolTokenWithdraw
          votingPower
          gasPrice
          commissionRate
          nextEpochStake
          nextEpochGasPrice
          nextEpochCommissionRate
          # todo DVX-1697 populate atRisk
          atRisk
        }
      }
    }
  }
}

fragment VC on ValidatorCredentials {
  protocolPubKey
  networkPubKey
  workerPubKey
  proofOfPossession
  netAddress
  p2PAddress
  primaryAddress
  workerAddress
}
