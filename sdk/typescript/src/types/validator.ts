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

// APY Response
export const Apy = object({
  apy: number(),
  address: SuiAddress,
});

export const ValidatorsApy = object({
  epoch: string(),
  apys: array(Apy),
});

export type ValidatorsApy = Infer<typeof ValidatorsApy>;

// Staking
export const Balance = object({
  value: number(),
});

export const StakeObject = object({
  stakedSuiId: ObjectId,
  stakeRequestEpoch: EpochId,
  stakeActiveEpoch: EpochId,
  principal: string(),
  status: union([literal('Active'), literal('Pending'), literal('Unstaked')]),
  estimatedReward: optional(string()),
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

export const Validators = array(tuple([AuthorityName, string()]));

export const CommitteeInfo = object({
  epoch: EpochId,
  /** Array of (validator public key, stake unit) tuple */
  validators: Validators,
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
  votingPower: string(),
  gasPrice: string(),
  commissionRate: string(),
  nextEpochStake: string(),
  nextEpochGasPrice: string(),
  nextEpochCommissionRate: string(),
  stakingPoolId: string(),
  stakingPoolActivationEpoch: nullable(string()),
  stakingPoolDeactivationEpoch: nullable(string()),
  stakingPoolSuiBalance: string(),
  rewardsPool: string(),
  poolTokenBalance: string(),
  pendingStake: string(),
  pendingPoolTokenWithdraw: string(),
  pendingTotalSuiWithdraw: string(),
  exchangeRatesId: string(),
  exchangeRatesSize: string(),
});

export type SuiValidatorSummary = Infer<typeof SuiValidatorSummary>;

export const SuiSystemStateSummary = object({
  epoch: string(),
  protocolVersion: string(),
  systemStateVersion: string(),
  storageFundTotalObjectStorageRebates: string(),
  storageFundNonRefundableBalance: string(),
  referenceGasPrice: string(),
  safeMode: boolean(),
  safeModeStorageRewards: string(),
  safeModeComputationRewards: string(),
  safeModeStorageRebates: string(),
  safeModeNonRefundableStorageFee: string(),
  epochStartTimestampMs: string(),
  epochDurationMs: string(),
  stakeSubsidyStartEpoch: string(),
  maxValidatorCount: string(),
  minValidatorJoiningStake: string(),
  validatorLowStakeThreshold: string(),
  validatorVeryLowStakeThreshold: string(),
  validatorLowStakeGracePeriod: string(),
  stakeSubsidyBalance: string(),
  stakeSubsidyDistributionCounter: string(),
  stakeSubsidyCurrentDistributionAmount: string(),
  stakeSubsidyPeriodLength: string(),
  stakeSubsidyDecreaseRate: number(),
  totalStake: string(),
  activeValidators: array(SuiValidatorSummary),
  pendingActiveValidatorsId: string(),
  pendingActiveValidatorsSize: string(),
  pendingRemovals: array(string()),
  stakingPoolMappingsId: string(),
  stakingPoolMappingsSize: string(),
  inactivePoolsId: string(),
  inactivePoolsSize: string(),
  validatorCandidatesId: string(),
  validatorCandidatesSize: string(),
  atRiskValidators: array(tuple([SuiAddress, string()])),
  validatorReportRecords: array(tuple([SuiAddress, array(SuiAddress)])),
});

export type SuiSystemStateSummary = Infer<typeof SuiSystemStateSummary>;
