// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type Checkpoint = {
	epoch: string;
	sequenceNumber: string;
	digest: string;
	networkTotalTransactions: string;
	previousDigest?: string;
	epochRollingGasCostSummary: GasCostSummary;
	timestampMs: string;
	endOfEpochData?: EndOfEpochData;
	validatorSignature: string;
	transactions: string[];
	checkpointCommitments: any[];
};

export type GasCostSummary = {
	computationCost: string;
	storageCost: string;
	storageRebate: string;
	nonRefundableStorageFee: string;
};

export type EndOfEpochData = {
	nextEpochCommittee: [string, string][];
	nextEpochProtocolVersion: string;
	epochCommitments: any[];
};

export type CommitteeInfo = {
	epoch: string;
	/** Array of (validator public key, stake unit) tuple */
	validators: Validators;
};

export type Validators = [string, string][];

export type SuiSystemStateSummary = {
	epoch: string;
	protocolVersion: string;
	systemStateVersion: string;
	storageFundTotalObjectStorageRebates: string;
	storageFundNonRefundableBalance: string;
	referenceGasPrice: string;
	safeMode: boolean;
	safeModeStorageRewards: string;
	safeModeComputationRewards: string;
	safeModeStorageRebates: string;
	safeModeNonRefundableStorageFee: string;
	epochStartTimestampMs: string;
	epochDurationMs: string;
	stakeSubsidyStartEpoch: string;
	maxValidatorCount: string;
	minValidatorJoiningStake: string;
	validatorLowStakeThreshold: string;
	validatorVeryLowStakeThreshold: string;
	validatorLowStakeGracePeriod: string;
	stakeSubsidyBalance: string;
	stakeSubsidyDistributionCounter: string;
	stakeSubsidyCurrentDistributionAmount: string;
	stakeSubsidyPeriodLength: string;
	stakeSubsidyDecreaseRate: number;
	totalStake: string;
	activeValidators: SuiValidatorSummary[];
	pendingActiveValidatorsId: string;
	pendingActiveValidatorsSize: string;
	pendingRemovals: string[];
	stakingPoolMappingsId: string;
	stakingPoolMappingsSize: string;
	inactivePoolsId: string;
	inactivePoolsSize: string;
	validatorCandidatesId: string;
	validatorCandidatesSize: string;
	atRiskValidators: [string, string][];
	validatorReportRecords: [string, string[]][];
};

export type SuiValidatorSummary = {
	suiAddress: string;
	protocolPubkeyBytes: string;
	networkPubkeyBytes: string;
	workerPubkeyBytes: string;
	proofOfPossessionBytes: string;
	operationCapId: string;
	name: string;
	description: string;
	imageUrl: string;
	projectUrl: string;
	p2pAddress: string;
	netAddress: string;
	primaryAddress: string;
	workerAddress: string;
	nextEpochProtocolPubkeyBytes: string | null;
	nextEpochProofOfPossession: string | null;
	nextEpochNetworkPubkeyBytes: string | null;
	nextEpochWorkerPubkeyBytes: string | null;
	nextEpochNetAddress: string | null;
	nextEpochP2pAddress: string | null;
	nextEpochPrimaryAddress: string | null;
	nextEpochWorkerAddress: string | null;
	votingPower: string;
	gasPrice: string;
	commissionRate: string;
	nextEpochStake: string;
	nextEpochGasPrice: string;
	nextEpochCommissionRate: string;
	stakingPoolId: string;
	stakingPoolActivationEpoch: string | null;
	stakingPoolDeactivationEpoch: string | null;
	stakingPoolSuiBalance: string;
	rewardsPool: string;
	poolTokenBalance: string;
	pendingStake: string;
	pendingPoolTokenWithdraw: string;
	pendingTotalSuiWithdraw: string;
	exchangeRatesId: string;
	exchangeRatesSize: string;
};

export type CheckpointedObjectId = {
	objectId: string;
	atCheckpoint?: number;
};

export type ValidatorsApy = {
	epoch: string;
	apys: Apy[];
};

// APY Response
export type Apy = {
	apy: number;
	address: string;
};

export type ResolvedNameServiceNames = {
	data: string[];
	hasNextPage: boolean;
	nextCursor: string | null;
};

export type ProtocolConfig = {
	attributes: Record<string, ProtocolConfigValue | null>;
	featureFlags: Record<string, boolean>;
	maxSupportedProtocolVersion: string;
	minSupportedProtocolVersion: string;
	protocolVersion: string;
};

export type ProtocolConfigValue = { u32: string } | { u64: string } | { f64: string };

export type EpochInfo = {
	epoch: string;
	validators: SuiValidatorSummary[];
	epochTotalTransactions: string;
	firstCheckpointId: string;
	epochStartTimestamp: string;
	endOfEpochInfo: EndOfEpochInfo | null;
	referenceGasPrice: number | null;
};

export type EndOfEpochInfo = {
	lastCheckpointId: string;
	epochEndTimestamp: string;
	protocolVersion: string;
	referenceGasPrice: string;
	totalStake: string;
	storageFundReinvestment: string;
	storageCharge: string;
	storageRebate: string;
	storageFundBalance: string;
	stakeSubsidyAmount: string;
	totalGasFees: string;
	totalStakeRewardsDistributed: string;
	leftoverStorageFundInflow: string;
};

export type EpochPage = {
	data: EpochInfo[];
	nextCursor: string | null;
	hasNextPage: boolean;
};

export type CheckpointPage = {
	data: Checkpoint[];
	nextCursor: string | null;
	hasNextPage: boolean;
};

export type NetworkMetrics = {
	currentTps: number;
	tps30Days: number;
	currentCheckpoint: string;
	currentEpoch: string;
	totalAddresses: string;
	totalObjects: string;
	totalPackages: string;
};

export type AddressMetrics = {
	checkpoint: number;
	epoch: number;
	timestampMs: number;
	cumulativeAddresses: number;
	cumulativeActiveAddresses: number;
	dailyActiveAddresses: number;
};

export type AllEpochsAddressMetrics = AddressMetrics[];
