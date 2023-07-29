// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import {
	array,
	boolean,
	literal,
	number,
	object,
	string,
	union,
	nullable,
	tuple,
	optional,
} from 'superstruct';

/* -------------- Types for the SuiSystemState Rust definition -------------- */

export type DelegatedStake = Infer<typeof DelegatedStake>;
export type CommitteeInfo = Infer<typeof CommitteeInfo>;
export type StakeObject = Infer<typeof StakeObject>;

// APY Response
export const Apy = object({
	apy: number(),
	address: string(),
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
	stakedSuiId: string(),
	stakeRequestEpoch: string(),
	stakeActiveEpoch: string(),
	principal: string(),
	status: union([literal('Active'), literal('Pending'), literal('Unstaked')]),
	estimatedReward: optional(string()),
});

export const DelegatedStake = object({
	validatorAddress: string(),
	stakingPool: string(),
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

export const Validators = array(tuple([string(), string()]));

export const CommitteeInfo = object({
	epoch: string(),
	/** Array of (validator public key, stake unit) tuple */
	validators: Validators,
});

export const SuiValidatorSummary = object({
	suiAddress: string(),
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
	atRiskValidators: array(tuple([string(), string()])),
	validatorReportRecords: array(tuple([string(), array(string())])),
});

export type SuiSystemStateSummary = Infer<typeof SuiSystemStateSummary>;
