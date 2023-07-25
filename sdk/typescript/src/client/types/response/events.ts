// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ObjectOwner } from './objects.js';

// event types mirror those in "sui-json-rpc-types/src/sui_event.rs"
export type SuiEvent = {
	id: EventId;
	// Move package where this event was emitted.
	packageId: string;
	// Move module where this event was emitted.
	transactionModule: string;
	// Sender's Sui address.
	sender: string;
	// Move event type.
	type: string;
	// Parsed json value of the event
	parsedJson?: Record<string, any>;
	// Base 58 encoded bcs bytes of the move event
	bcs?: string;
	timestampMs?: string;
};

export type EventId = {
	txDigest: string;
	eventSeq: string;
};

export type PaginatedEvents = {
	data: SuiEvent[];
	nextCursor: EventId | null;
	hasNextPage: boolean;
};

export type BalanceChange = {
	owner: ObjectOwner;
	coinType: string;
	/* Coin balance change(positive means receive, negative means send) */
	amount: string;
};

export type SuiObjectChangePublished = {
	type: 'published';
	packageId: string;
	version: string;
	digest: string;
	modules: string[];
};

export type SuiObjectChangeTransferred = {
	type: 'transferred';
	sender: string;
	recipient: ObjectOwner;
	objectType: string;
	objectId: string;
	version: string;
	digest: string;
};

export type SuiObjectChangeMutated = {
	type: 'mutated';
	sender: string;
	owner: ObjectOwner;
	objectType: string;
	objectId: string;
	version: string;
	previousVersion: string;
	digest: string;
};

export type SuiObjectChangeDeleted = {
	type: 'deleted';
	sender: string;
	objectType: string;
	objectId: string;
	version: string;
};

export type SuiObjectChangeWrapped = {
	type: 'wrapped';
	sender: string;
	objectType: string;
	objectId: string;
	version: string;
};

export type SuiObjectChangeCreated = {
	type: 'created';
	sender: string;
	owner: ObjectOwner;
	objectType: string;
	objectId: string;
	version: string;
	digest: string;
};

export type SuiObjectChange =
	| SuiObjectChangePublished
	| SuiObjectChangeTransferred
	| SuiObjectChangeMutated
	| SuiObjectChangeDeleted
	| SuiObjectChangeWrapped
	| SuiObjectChangeCreated;
