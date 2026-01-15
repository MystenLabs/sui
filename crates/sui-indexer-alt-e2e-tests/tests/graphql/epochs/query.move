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
    contents {
      totalStake: format(format: "{total_stake:json}")
      pendingRemovals: format(format: "{pending_removals:json}")
      pendingActiveValidators: format(format: "{pending_active_validators:json}")
      stakingPoolMappings: format(format: "{staking_pool_mappings:json}")
      inactiveValidators: format(format: "{inactive_validators:json}")
      validatorCandidates: format(format: "{validator_candidates:json}")
    }
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
  systemState {
    protocolVersion: format(format: "{protocol_version:json}")
    systemStateVersion: format(format: "{system_state_version:json}")
    storageFund: format(format: "{storage_fund:json}")
    parameters: format(format: "{parameters:json}")
    stakeSubsidy: format(format: "{stake_subsidy:json}")
    safeMode: format(format: "{safe_mode:json}")
    safeModeComputationRewards: format(format: "{safe_mode_computation_rewards:json}")
    safeModeStorageRewards: format(format: "{safe_mode_storage_rewards:json}")
    safeModeStorageRebates: format(format: "{safe_mode_storage_rebates:json}")
    safeModeNonRefundableStorageFee: format(format: "{safe_mode_non_refundable_storage_fee:json}")
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
    contents {
      totalStake: format(format: "{total_stake:json}")
      pendingRemovals: format(format: "{pending_removals:json}")
      pendingActiveValidators: format(format: "{pending_active_validators:json}")
      stakingPoolMappings: format(format: "{staking_pool_mappings:json}")
      inactiveValidators: format(format: "{inactive_validators:json}")
      validatorCandidates: format(format: "{validator_candidates:json}")
    }
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
  systemState {
    protocolVersion: format(format: "{protocol_version:json}")
    systemStateVersion: format(format: "{system_state_version:json}")
    storageFund: format(format: "{storage_fund:json}")
    parameters: format(format: "{parameters:json}")
    stakeSubsidy: format(format: "{stake_subsidy:json}")
    safeMode: format(format: "{safe_mode:json}")
    safeModeComputationRewards: format(format: "{safe_mode_computation_rewards:json}")
    safeModeStorageRewards: format(format: "{safe_mode_storage_rewards:json}")
    safeModeStorageRebates: format(format: "{safe_mode_storage_rebates:json}")
    safeModeNonRefundableStorageFee: format(format: "{safe_mode_non_refundable_storage_fee:json}")
  }
  liveObjectSetDigest
}
