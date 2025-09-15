// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# run-graphql
{
  epoch(epochId: 0) { ...E }
}

fragment E on Epoch {
  epochId
  validatorSet {
    activeValidators {
      address
      credentials { ...VC }
      # todo (ewall) populate nextEpochCredentials
      nextEpochCredentials { ...VC }
      name
      # todo (ewall) populate description
      description
      # todo (ewall) populate imageUrl
      imageUrl
      # todo (ewall) populate projectUrl
      projectUrl
      stakingPoolId
      exchangeRatesSize
      stakingPoolActivationEpoch
      stakingPoolSuiBalance
      # todo (ewall) populate rewardsPool
      rewardsPool
      poolTokenBalance
      # todo (ewall) populate pendingStake
      pendingStake
      # todo (ewall) populate pendingTotalSuiWithdraw
      pendingTotalSuiWithdraw
      # todo (ewall) populate pendingPoolTokenWithdraw
      pendingPoolTokenWithdraw
      votingPower
      gasPrice
      commissionRate
      nextEpochStake
      nextEpochGasPrice
      nextEpochCommissionRate
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
