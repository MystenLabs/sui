// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# run-graphql
{ # todo create test with safeMode enabled
  e0: epoch(epochId: 0) {
    systemState {
      safeMode: format(format: "{safe_mode:json}")
      safeModeComputationRewards: format(format: "{safe_mode_computation_rewards:json}")
      safeModeStorageRewards: format(format: "{safe_mode_storage_rewards:json}")
      safeModeStorageRebates: format(format: "{safe_mode_storage_rebates:json}")
      safeModeNonRefundableStorageFee: format(format: "{safe_mode_non_refundable_storage_fee:json}")
    }
  }
}
