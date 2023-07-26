// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiJsonValue } from '../common.js';
import type { GasCostSummary } from './chain.js';
import type { BalanceChange, SuiEvent, SuiObjectChange } from './events.js';
import type { SuiMovePackage } from './move.js';
import type { OwnedObjectRef, SuiObjectRef } from './objects.js';

export type TransactionEffects = {
	// Eventually this will become union(literal('v1'), literal('v2'), ...)
	messageVersion: 'v1';

	/** The status of the execution */
	status: ExecutionStatus;
	/** The epoch when this transaction was executed */
	executedEpoch: string;
	/** The version that every modified (mutated or deleted) object had before it was modified by this transaction. **/
	modifiedAtVersions?: TransactionEffectsModifiedAtVersions[];
	gasUsed: GasCostSummary;
	/** The object references of the shared objects used in this transaction. Empty if no shared objects were used. */
	sharedObjects?: SuiObjectRef[];
	/** The transaction digest */
	transactionDigest: string;
	/** ObjectRef and owner of new objects created */
	created?: OwnedObjectRef[];
	/** ObjectRef and owner of mutated objects, including gas object */
	mutated?: OwnedObjectRef[];
	/**
	 * ObjectRef and owner of objects that are unwrapped in this transaction.
	 * Unwrapped objects are objects that were wrapped into other objects in the past,
	 * and just got extracted out.
	 */
	unwrapped?: OwnedObjectRef[];
	/** Object Refs of objects now deleted (the old refs) */
	deleted?: SuiObjectRef[];
	/** Object Refs of objects now deleted (the old refs) */
	unwrappedThenDeleted?: SuiObjectRef[];
	/** Object refs of objects now wrapped in other objects */
	wrapped?: SuiObjectRef[];
	/**
	 * The updated gas object reference. Have a dedicated field for convenient access.
	 * It's also included in mutated.
	 */
	gasObject: OwnedObjectRef;
	/** The events emitted during execution. Note that only successful transactions emit events */
	eventsDigest?: string;
	/** The set of transaction digests this transaction depends on */
	dependencies?: string[];
};

export type ExecutionStatus = {
	status: ExecutionStatusType;
	error?: string;
};

export type ExecutionStatusType = 'success' | 'failure';

export type TransactionEffectsModifiedAtVersions = {
	objectId: string;
	sequenceNumber: string;
};

export type PaginatedTransactionResponse = {
	data: SuiTransactionBlockResponse[];
	nextCursor: string | null;
	hasNextPage: boolean;
};

export type SuiTransactionBlockResponse = {
	digest: string;
	transaction?: SuiTransactionBlock;
	effects?: TransactionEffects;
	events?: SuiEvent[];
	timestampMs?: string;
	checkpoint?: string;
	confirmedLocalExecution?: boolean;
	objectChanges?: SuiObjectChange[];
	balanceChanges?: BalanceChange[];
	/* Errors that occurred in fetching/serializing the transaction. */
	errors?: string[];
};

export type SuiTransactionBlock = {
	data: SuiTransactionBlockData;
	txSignatures: string[];
};

export type SuiTransactionBlockData = {
	// Eventually this will become union(literal('v1'), literal('v2'), ...)
	messageVersion: 'v1';
	transaction: SuiTransactionBlockKind;
	sender: string;
	gasData: SuiGasData;
};

export type SuiTransactionBlockKind =
	| (SuiChangeEpoch & { kind: 'ChangeEpoch' })
	| (SuiConsensusCommitPrologue & {
			kind: 'ConsensusCommitPrologue';
	  })
	| (Genesis & { kind: 'Genesis' })
	| (ProgrammableTransaction & { kind: 'ProgrammableTransaction' });

export type SuiChangeEpoch = {
	epoch: string;
	storage_charge: string;
	computation_charge: string;
	storage_rebate: string;
	epoch_start_timestamp_ms?: string;
};

export type Genesis = {
	objects: string[];
};

export type ProgrammableTransaction = {
	transactions: SuiTransaction[];
	inputs: SuiCallArg[];
};

export type SuiCallArg =
	| {
			type: 'pure';
			valueType: string | null;
			value: SuiJsonValue;
	  }
	| {
			type: 'object';
			objectType: 'immOrOwnedObject';
			objectId: string;
			version: string;
			digest: string;
	  }
	| {
			type: 'object';
			objectType: 'sharedObject';
			objectId: string;
			initialSharedVersion: string;
			mutable: boolean;
	  };

export type SuiTransaction =
	| { MoveCall: MoveCallSuiTransaction }
	| { TransferObjects: [SuiArgument[], SuiArgument] }
	| { SplitCoins: [SuiArgument, SuiArgument[]] }
	| { MergeCoins: [SuiArgument, SuiArgument[]] }
	| {
			Publish: // TODO: Remove this after 0.34 is released:
			[SuiMovePackage, string[]] | string[];
	  }
	| {
			Upgrade: // TODO: Remove this after 0.34 is released:
			[SuiMovePackage, string[], string, SuiArgument] | [string[], string, SuiArgument];
	  }
	| { MakeMoveVec: [string | null, SuiArgument[]] };

export type SuiArgument =
	| 'GasCoin'
	| { Input: number }
	| { Result: number }
	| { NestedResult: [number, number] };

export type MoveCallSuiTransaction = {
	arguments?: SuiArgument[];
	type_arguments?: string[];
	package: string;
	module: string;
	function: string;
};

export type SuiGasData = {
	payment: SuiObjectRef[];
	/** Gas Object's owner */
	owner: string;
	price: string;
	budget: string;
};

export type ExecutionResultType = {
	mutableReferenceOutputs?: MutableReferenceOutputType[];
	returnValues?: ReturnValueType[];
};

export type ReturnValueType = [number[], string];

export type MutableReferenceOutputType = [SuiArgument, number[], string];

export type SuiConsensusCommitPrologue = {
	epoch: string;
	round: string;
	commit_timestamp_ms: string;
};

export type DryRunTransactionBlockResponse = {
	effects: TransactionEffects;
	events: SuiEvent[];
	objectChanges: SuiObjectChange[];
	balanceChanges: BalanceChange[];
	// TODO: Remove optional when this is rolled out to all networks:
	input?: SuiTransactionBlockData;
};

export type DevInspectResults = {
	effects: TransactionEffects;
	events: SuiEvent[];
	results?: ExecutionResultType[];
	error?: string;
};
