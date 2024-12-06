// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/bcs';

import { Address, ObjectDigest, SuiObjectRef } from './bcs.js';

const PackageUpgradeError = bcs.enum('PackageUpgradeError', {
	UnableToFetchPackage: bcs.struct('UnableToFetchPackage', { packageId: Address }),
	NotAPackage: bcs.struct('NotAPackage', { objectId: Address }),
	IncompatibleUpgrade: null,
	DigestDoesNotMatch: bcs.struct('DigestDoesNotMatch', { digest: bcs.vector(bcs.u8()) }),
	UnknownUpgradePolicy: bcs.struct('UnknownUpgradePolicy', { policy: bcs.u8() }),
	PackageIDDoesNotMatch: bcs.struct('PackageIDDoesNotMatch', {
		packageId: Address,
		ticketId: Address,
	}),
});

const ModuleId = bcs.struct('ModuleId', {
	address: Address,
	name: bcs.string(),
});
const MoveLocation = bcs.struct('MoveLocation', {
	module: ModuleId,
	function: bcs.u16(),
	instruction: bcs.u16(),
	functionName: bcs.option(bcs.string()),
});

const CommandArgumentError = bcs.enum('CommandArgumentError', {
	TypeMismatch: null,
	InvalidBCSBytes: null,
	InvalidUsageOfPureArg: null,
	InvalidArgumentToPrivateEntryFunction: null,
	IndexOutOfBounds: bcs.struct('IndexOutOfBounds', { idx: bcs.u16() }),
	SecondaryIndexOutOfBounds: bcs.struct('SecondaryIndexOutOfBounds', {
		resultIdx: bcs.u16(),
		secondaryIdx: bcs.u16(),
	}),
	InvalidResultArity: bcs.struct('InvalidResultArity', { resultIdx: bcs.u16() }),
	InvalidGasCoinUsage: null,
	InvalidValueUsage: null,
	InvalidObjectByValue: null,
	InvalidObjectByMutRef: null,
	SharedObjectOperationNotAllowed: null,
});

const TypeArgumentError = bcs.enum('TypeArgumentError', {
	TypeNotFound: null,
	ConstraintNotSatisfied: null,
});

const ExecutionFailureStatus = bcs.enum('ExecutionFailureStatus', {
	InsufficientGas: null,
	InvalidGasObject: null,
	InvariantViolation: null,
	FeatureNotYetSupported: null,
	MoveObjectTooBig: bcs.struct('MoveObjectTooBig', {
		objectSize: bcs.u64(),
		maxObjectSize: bcs.u64(),
	}),
	MovePackageTooBig: bcs.struct('MovePackageTooBig', {
		objectSize: bcs.u64(),
		maxObjectSize: bcs.u64(),
	}),
	CircularObjectOwnership: bcs.struct('CircularObjectOwnership', { object: Address }),
	InsufficientCoinBalance: null,
	CoinBalanceOverflow: null,
	PublishErrorNonZeroAddress: null,
	SuiMoveVerificationError: null,
	MovePrimitiveRuntimeError: bcs.option(MoveLocation),
	MoveAbort: bcs.tuple([MoveLocation, bcs.u64()]),
	VMVerificationOrDeserializationError: null,
	VMInvariantViolation: null,
	FunctionNotFound: null,
	ArityMismatch: null,
	TypeArityMismatch: null,
	NonEntryFunctionInvoked: null,
	CommandArgumentError: bcs.struct('CommandArgumentError', {
		argIdx: bcs.u16(),
		kind: CommandArgumentError,
	}),
	TypeArgumentError: bcs.struct('TypeArgumentError', {
		argumentIdx: bcs.u16(),
		kind: TypeArgumentError,
	}),
	UnusedValueWithoutDrop: bcs.struct('UnusedValueWithoutDrop', {
		resultIdx: bcs.u16(),
		secondaryIdx: bcs.u16(),
	}),
	InvalidPublicFunctionReturnType: bcs.struct('InvalidPublicFunctionReturnType', {
		idx: bcs.u16(),
	}),
	InvalidTransferObject: null,
	EffectsTooLarge: bcs.struct('EffectsTooLarge', { currentSize: bcs.u64(), maxSize: bcs.u64() }),
	PublishUpgradeMissingDependency: null,
	PublishUpgradeDependencyDowngrade: null,
	PackageUpgradeError: bcs.struct('PackageUpgradeError', { upgradeError: PackageUpgradeError }),
	WrittenObjectsTooLarge: bcs.struct('WrittenObjectsTooLarge', {
		currentSize: bcs.u64(),
		maxSize: bcs.u64(),
	}),
	CertificateDenied: null,
	SuiMoveVerificationTimedout: null,
	SharedObjectOperationNotAllowed: null,
	InputObjectDeleted: null,
	ExecutionCancelledDueToSharedObjectCongestion: bcs.struct(
		'ExecutionCancelledDueToSharedObjectCongestion',
		{
			congestedObjects: bcs.vector(Address),
		},
	),
	AddressDeniedForCoin: bcs.struct('AddressDeniedForCoin', {
		address: Address,
		coinType: bcs.string(),
	}),
	CoinTypeGlobalPause: bcs.struct('CoinTypeGlobalPause', { coinType: bcs.string() }),
	ExecutionCancelledDueToRandomnessUnavailable: null,
});

const ExecutionStatus = bcs.enum('ExecutionStatus', {
	Success: null,
	Failed: bcs.struct('ExecutionFailed', {
		error: ExecutionFailureStatus,
		command: bcs.option(bcs.u64()),
	}),
});

const GasCostSummary = bcs.struct('GasCostSummary', {
	computationCost: bcs.u64(),
	storageCost: bcs.u64(),
	storageRebate: bcs.u64(),
	nonRefundableStorageFee: bcs.u64(),
});

const Owner = bcs.enum('Owner', {
	AddressOwner: Address,
	ObjectOwner: Address,
	Shared: bcs.struct('Shared', {
		initialSharedVersion: bcs.u64(),
	}),
	Immutable: null,
});

const TransactionEffectsV1 = bcs.struct('TransactionEffectsV1', {
	status: ExecutionStatus,
	executedEpoch: bcs.u64(),
	gasUsed: GasCostSummary,
	modifiedAtVersions: bcs.vector(bcs.tuple([Address, bcs.u64()])),
	sharedObjects: bcs.vector(SuiObjectRef),
	transactionDigest: ObjectDigest,
	created: bcs.vector(bcs.tuple([SuiObjectRef, Owner])),
	mutated: bcs.vector(bcs.tuple([SuiObjectRef, Owner])),
	unwrapped: bcs.vector(bcs.tuple([SuiObjectRef, Owner])),
	deleted: bcs.vector(SuiObjectRef),
	unwrappedThenDeleted: bcs.vector(SuiObjectRef),
	wrapped: bcs.vector(SuiObjectRef),
	gasObject: bcs.tuple([SuiObjectRef, Owner]),
	eventsDigest: bcs.option(ObjectDigest),
	dependencies: bcs.vector(ObjectDigest),
});

const VersionDigest = bcs.tuple([bcs.u64(), ObjectDigest]);

const ObjectIn = bcs.enum('ObjectIn', {
	NotExist: null,
	Exist: bcs.tuple([VersionDigest, Owner]),
});

const ObjectOut = bcs.enum('ObjectOut', {
	NotExist: null,
	ObjectWrite: bcs.tuple([ObjectDigest, Owner]),
	PackageWrite: VersionDigest,
});

const IDOperation = bcs.enum('IDOperation', {
	None: null,
	Created: null,
	Deleted: null,
});

const EffectsObjectChange = bcs.struct('EffectsObjectChange', {
	inputState: ObjectIn,
	outputState: ObjectOut,
	idOperation: IDOperation,
});

const UnchangedSharedKind = bcs.enum('UnchangedSharedKind', {
	ReadOnlyRoot: VersionDigest,
	MutateDeleted: bcs.u64(),
	ReadDeleted: bcs.u64(),
	Cancelled: bcs.u64(),
	PerEpochConfig: null,
});

const TransactionEffectsV2 = bcs.struct('TransactionEffectsV2', {
	status: ExecutionStatus,
	executedEpoch: bcs.u64(),
	gasUsed: GasCostSummary,
	transactionDigest: ObjectDigest,
	gasObjectIndex: bcs.option(bcs.u32()),
	eventsDigest: bcs.option(ObjectDigest),
	dependencies: bcs.vector(ObjectDigest),
	lamportVersion: bcs.u64(),
	changedObjects: bcs.vector(bcs.tuple([Address, EffectsObjectChange])),
	unchangedSharedObjects: bcs.vector(bcs.tuple([Address, UnchangedSharedKind])),
	auxDataDigest: bcs.option(ObjectDigest),
});

export const TransactionEffects = bcs.enum('TransactionEffects', {
	V1: TransactionEffectsV1,
	V2: TransactionEffectsV2,
});
