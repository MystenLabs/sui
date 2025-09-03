// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# advance-clock --duration-ns 123000000

//# advance-epoch

//# advance-clock --duration-ns 321000000

//# advance-epoch

//# advance-epoch

//# run-graphql
{
  latest: epoch { ...E }

  e0: epoch(epochId: 0) { ...E }
  e1: epoch(epochId: 1) { ...E }
  e2: epoch(epochId: 2) { ...E }

  # This epoch doesn't exist yet
  e3: epoch(epochId: 3) { ...E }
}

fragment E on Epoch {
  epochId
  referenceGasPrice
  startTimestamp
  endTimestamp
  validatorSet {
    totalStake
    pendingRemovals
    pendingActiveValidatorsId
    pendingActiveValidatorsSize
    stakingPoolMappingsId
    stakingPoolMappingsSize
    inactivePoolsId
    inactivePoolsSize
    validatorCandidatesId
    validatorCandidatesSize
  }
  totalCheckpoints
  totalTransactions
  totalGasFees
  totalStakeRewards
  totalStakeSubsidies
  fundSize
  netInflow
  fundInflow
  fundOutflow
  storageFund {
    totalObjectStorageRebates
    nonRefundableBalance
  }
  safeMode {
    enabled
    gasSummary {
      computationCost
      storageCost
      storageRebate
      nonRefundableStorageFee
    }
  }
  systemStateVersion
  systemParameters {
    durationMs
    stakeSubsidyStartEpoch
    minValidatorCount
    maxValidatorCount
    minValidatorJoiningStake
    validatorLowStakeThreshold
    validatorVeryLowStakeThreshold
    validatorLowStakeGracePeriod
  }
  systemStakeSubsidy {
      balance
      distributionCounter
      currentDistributionAmount
      periodLength
      decreaseRate
  }
  liveObjectSetDigest
}

//# run-graphql
{ # This checkpoint is half way through an earlier epoch, which should be
  # reflected in the latest epoch we get a start time for and an end time for.
  checkpoint(sequenceNumber: 2) {
    query {
      latest: epoch { ...E }
      e0: epoch(epochId: 0) { ...E }
      e1: epoch(epochId: 1) { ...E }
      e2: epoch(epochId: 2) { ...E }
    }
  }
}

fragment E on Epoch {
  epochId
  referenceGasPrice
  startTimestamp
  endTimestamp
  validatorSet {
    totalStake
    pendingRemovals
    pendingActiveValidatorsId
    pendingActiveValidatorsSize
    stakingPoolMappingsId
    stakingPoolMappingsSize
    inactivePoolsId
    inactivePoolsSize
    validatorCandidatesId
    validatorCandidatesSize
  }
  totalCheckpoints
  totalTransactions
  totalGasFees
  totalStakeRewards
  totalStakeSubsidies
  fundSize
  netInflow
  fundInflow
  fundOutflow
  storageFund {
    totalObjectStorageRebates
    nonRefundableBalance
  }
  safeMode {
    enabled
    gasSummary {
      computationCost
      storageCost
      storageRebate
      nonRefundableStorageFee
    }
  }
  systemStateVersion
  systemParameters {
    durationMs
    stakeSubsidyStartEpoch
    minValidatorCount
    maxValidatorCount
    minValidatorJoiningStake
    validatorLowStakeThreshold
    validatorVeryLowStakeThreshold
    validatorLowStakeGracePeriod
  }
  systemStakeSubsidy {
      balance
      distributionCounter
      currentDistributionAmount
      periodLength
      decreaseRate
  }
  liveObjectSetDigest
}
