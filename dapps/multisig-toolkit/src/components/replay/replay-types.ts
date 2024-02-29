// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type ReplayType = {
	effects: Effects;
	gasStatus: ReplayGasStatus;
	transactionInfo: TransactionInfo;
};

export type Effects = {
	messageVersion: string;
	status: Status;
	executedEpoch: string;
	gasUsed: GasUsed;
	modifiedAtVersions: ModifiedAtVersion[];
	sharedObjects: Reference[];
	transactionDigest: string;
	mutated: Mutated[];
	gasObject: GasObject;
	dependencies: string[];
};

export type GasObject = {
	owner: GasObjectOwner;
	reference: Reference;
};

export type GasObjectOwner = {
	AddressOwner: string;
};

export type Reference = {
	objectId: string;
	version: number;
	digest: string;
};

export type GasUsed = {
	computationCost: string;
	storageCost: string;
	storageRebate: string;
	nonRefundableStorageFee: string;
};

export type ModifiedAtVersion = {
	objectId: string;
	sequenceNumber: string;
};

export type Mutated = {
	owner: MutatedOwner;
	reference: Reference;
};

export type MutatedOwner = {
	ObjectOwner?: string;
	Shared?: Shared;
	AddressOwner?: string;
};

export type Shared = {
	initialSharedVersion: number;
};

export type Status = {
	status: string;
};

export type ReplayGasStatus = {
	V2: V2;
};

export type V2 = {
	gasStatus: V2GasStatus;
	costTable: CostTable;
	gasBudget: number;
	computationCost: number;
	charge: boolean;
	gasPrice: number;
	referenceGasPrice: number;
	storageGasPrice: number;
	perObjectStorage: Array<PerObjectStorageElement[]>;
	rebateRate: number;
	unmeteredStorageRebate: number;
	gasRoundingStep: number;
};

export type CostTable = {
	minTransactionCost: number;
	maxGasBudget: number;
	packagePublishPerByteCost: number;
	objectReadPerByteCost: number;
	storagePerByteCost: number;
	executionCostTable: ExecutionCostTableClass;
	computationBucket: ComputationBucket[];
};

export type ComputationBucket = {
	min: number;
	max: number;
	cost: number;
};

export type ExecutionCostTableClass = {
	instructionTiers: { [key: string]: number };
	stackHeightTiers: { [key: string]: number };
	stackSizeTiers: { [key: string]: number };
};

export type V2GasStatus = {
	gasModelVersion: number;
	costTable: ExecutionCostTableClass;
	gasLeft: GasLeft;
	gasPrice: number;
	initialBudget: GasLeft;
	charge: boolean;
	stackHeightHighWaterMark: number;
	stackHeightCurrent: number;
	stackHeightNextTierStart: number;
	stackHeightCurrentTierMult: number;
	stackSizeHighWaterMark: number;
	stackSizeCurrent: number;
	stackSizeNextTierStart: number;
	stackSizeCurrentTierMult: number;
	instructionsExecuted: number;
	instructionsNextTierStart: number;
	instructionsCurrentTierMult: number;
	profiler: null;
};

export type GasLeft = {
	val: number;
	phantom: null;
};

export type PerObjectStorageElement = PerObjectStorageClass | string;

export type PerObjectStorageClass = {
	storageCost: number;
	storageRebate: number;
	newSize: number;
};

export type TransactionInfo = {
	ProgrammableTransaction: ReplayProgrammableTransactions;
};

export type ReplayProgrammableTransactions = {
	inputs: ReplayInput[];
	commands: Command[];
};

export type Command = {
	MoveCall: MoveCall;
	SplitCoins: [string | Argument, (string | Argument)[]];
	// MergeCoins, Publish, Upgrade, MakeMoveVec etc.
};

export type MoveCall = {
	package: string;
	module: string;
	function: string;
	typeArguments: any[];
	arguments: Argument[];
};

export type Argument = {
	Input?: number;
	NestedResult?: number[];
	Result?: number;
};

export type ReplayInput = {
	Object?: ReplayInputObject;
	Pure?: number[];
};

export type TypeArgument = {
	struct: Struct;
};

export type Struct = {
	address: string;
	module: string;
	name: string;
	type_args: any[];
};

export type ReplayInputObject = {
	ImmOrOwnedObject?: [string, number, string]; // id, version, digest
	SharedObject?: SharedObject;
};

export type SharedObject = {
	id: string;
	initialSharedVersion: number;
	mutable: boolean;
};
