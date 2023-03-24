// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  array,
  boolean,
  literal,
  number,
  object,
  string,
  union,
  Infer,
  nullable,
  tuple,
  optional,
} from 'superstruct';
import { ObjectId, SuiAddress } from './common';
import { AuthorityName, EpochId } from './transactions';

/* -------------- Types for the SuiSystemState Rust definition -------------- */

export type DelegatedStake = Infer<typeof DelegatedStake>;
export type CommitteeInfo = Infer<typeof CommitteeInfo>;
export type StakeObject = Infer<typeof StakeObject>;

// Staking

export const Balance = object({
  value: number(),
});

export const StakeObject = object({
  stakedSuiId: ObjectId,
  stakeRequestEpoch: EpochId,
  stakeActiveEpoch: EpochId,
  principal: number(),
  status: union([literal('Active'), literal('Pending'), literal('Unstaked')]),
  estimatedReward: optional(number()),
});

export const DelegatedStake = object({
  validatorAddress: SuiAddress,
  stakingPool: ObjectId,
  stakes: array(StakeObject),
});

export const StakeSubsidyFields = object({
  balance: object({ value: number() }),
  distribution_counter: number(),
  current_distribution_amount: number(),
  stake_subsidy_period_length: number(),
  stake_subsidy_decrease_rate: number(),
});

export const StakeSubsidy = object({
  type: string(),
  fields: StakeSubsidyFields,
});

export const SuiSupplyFields = object({
  value: number(),
});

export const ContentsFields = object({
  id: string(),
  size: number(),
  head: object({ vec: array() }),
  tail: object({ vec: array() }),
});

export const ContentsFieldsWithdraw = object({
  id: string(),
  size: number(),
});

export const Contents = object({
  type: string(),
  fields: ContentsFields,
});

export const DelegationStakingPoolFields = object({
  exchangeRates: object({
    id: string(),
    size: number(),
  }),
  id: string(),
  pendingStake: number(),
  pendingPoolTokenWithdraw: number(),
  pendingTotalSuiWithdraw: number(),
  poolTokenBalance: number(),
  rewardsPool: object({ value: number() }),
  activationEpoch: object({ vec: array() }),
  deactivationEpoch: object({ vec: array() }),
  suiBalance: number(),
});

export const DelegationStakingPool = object({
  type: string(),
  fields: DelegationStakingPoolFields,
});

export const CommitteeInfo = object({
  epoch: number(),
  /** Array of (validator public key, stake unit) tuple */
  validators: optional(array(tuple([AuthorityName, number()]))),
});

export const SuiValidatorSummary = object({
  suiAddress: SuiAddress,
  protocolPubkeyBytes: string(),
  networkPubkeyBytes: string(),
  workerPubkeyBytes: string(),
  proofOfPossessionBytes: string(),
  operationCapId: string(),
  name: string(),
  description: string(),
  imageUrl: string(),
  projectUrl: string(),
  p2pAddress: string(),
  netAddress: string(),
  primaryAddress: string(),
  workerAddress: string(),
  nextEpochProtocolPubkeyBytes: nullable(string()),
  nextEpochProofOfPossession: nullable(string()),
  nextEpochNetworkPubkeyBytes: nullable(string()),
  nextEpochWorkerPubkeyBytes: nullable(string()),
  nextEpochNetAddress: nullable(string()),
  nextEpochP2pAddress: nullable(string()),
  nextEpochPrimaryAddress: nullable(string()),
  nextEpochWorkerAddress: nullable(string()),
  votingPower: number(),
  gasPrice: number(),
  commissionRate: number(),
  nextEpochStake: number(),
  nextEpochGasPrice: number(),
  nextEpochCommissionRate: number(),
  stakingPoolId: string(),
  stakingPoolActivationEpoch: nullable(number()),
  stakingPoolDeactivationEpoch: nullable(number()),
  stakingPoolSuiBalance: number(),
  rewardsPool: number(),
  poolTokenBalance: number(),
  pendingStake: number(),
  pendingPoolTokenWithdraw: number(),
  pendingTotalSuiWithdraw: number(),
  exchangeRatesId: string(),
  exchangeRatesSize: number(),
});

export type SuiValidatorSummary = Infer<typeof SuiValidatorSummary>;

export const SuiSystemStateSummary = object({
  epoch: number(),
  protocolVersion: number(),
  systemStateVersion: number(),
  storageFundTotalObjectStorageRebates: number(),
  storageFundNonRefundableBalance: number(),
  referenceGasPrice: number(),
  safeMode: boolean(),
  safeModeStorageRewards: number(),
  safeModeComputationRewards: number(),
  safeModeStorageRebates: number(),
  safeModeNonRefundableStorageFee: number(),
  epochStartTimestampMs: number(),
  epochDurationMs: number(),
  stakeSubsidyStartEpoch: number(),
  maxValidatorCount: number(),
  minValidatorJoiningStake: number(),
  validatorLowStakeThreshold: number(),
  validatorVeryLowStakeThreshold: number(),
  validatorLowStakeGracePeriod: number(),
  stakeSubsidyBalance: number(),
  stakeSubsidyDistributionCounter: number(),
  stakeSubsidyCurrentDistributionAmount: number(),
  stakeSubsidyPeriodLength: number(),
  stakeSubsidyDecreaseRate: number(),
  totalStake: number(),
  activeValidators: array(SuiValidatorSummary),
  pendingActiveValidatorsId: string(),
  pendingActiveValidatorsSize: number(),
  pendingRemovals: array(number()),
  stakingPoolMappingsId: string(),
  stakingPoolMappingsSize: number(),
  inactivePoolsId: string(),
  inactivePoolsSize: number(),
  validatorCandidatesId: string(),
  validatorCandidatesSize: number(),
  atRiskValidators: array(tuple([SuiAddress, number()])),
  validatorReportRecords: array(tuple([SuiAddress, array(SuiAddress)])),
});

export type SuiSystemStateSummary = Infer<typeof SuiSystemStateSummary>;
