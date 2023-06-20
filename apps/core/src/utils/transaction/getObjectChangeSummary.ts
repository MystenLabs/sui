// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
	SuiObjectChangeTransferred,
	SuiObjectChangeCreated,
	SuiObjectChangeMutated,
	SuiObjectChangePublished,
	SuiObjectChange,
	DisplayFieldsResponse,
	SuiObjectChangeDeleted,
	SuiObjectChangeWrapped,
} from '@mysten/sui.js';
import { groupByOwner } from './groupByOwner';
import { SuiObjectChangeTypes } from './types';

export type WithDisplayFields<T> = T & { display?: DisplayFieldsResponse };
export type SuiObjectChangeWithDisplay = WithDisplayFields<SuiObjectChange>;

export type ObjectChanges = {
	changesWithDisplay: SuiObjectChangeWithDisplay[];
	changes: SuiObjectChange[];
	ownerType: string;
};
export type ObjectChangesByOwner = Record<string, ObjectChanges>;

export type ObjectChangeSummary = {
	[K in SuiObjectChangeTypes]: ObjectChangesByOwner;
};

export const getObjectChangeSummary = (objectChanges: SuiObjectChangeWithDisplay[]) => {
	if (!objectChanges) return null;

	const mutated = objectChanges.filter(
		(change) => change.type === 'mutated',
	) as SuiObjectChangeMutated[];

	const created = objectChanges.filter(
		(change) => change.type === 'created',
	) as SuiObjectChangeCreated[];

	const transferred = objectChanges.filter(
		(change) => change.type === 'transferred',
	) as SuiObjectChangeTransferred[];

	const published = objectChanges.filter(
		(change) => change.type === 'published',
	) as SuiObjectChangePublished[];

	const wrapped = objectChanges.filter(
		(change) => change.type === 'wrapped',
	) as SuiObjectChangeWrapped[];

	const deleted = objectChanges.filter(
		(change) => change.type === 'deleted',
	) as SuiObjectChangeDeleted[];

	return {
		transferred: groupByOwner(transferred),
		created: groupByOwner(created),
		mutated: groupByOwner(mutated),
		published: groupByOwner(published),
		wrapped: groupByOwner(wrapped),
		deleted: groupByOwner(deleted),
	};
};
