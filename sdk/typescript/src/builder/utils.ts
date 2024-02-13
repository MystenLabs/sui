// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Struct } from 'superstruct';
import { create as superstructCreate } from 'superstruct';

import type { SuiMoveNormalizedType } from '../client/index.js';

export function create<T, S>(value: T, struct: Struct<T, S>): T {
	return superstructCreate(value, struct);
}

export function extractMutableReference(
	normalizedType: SuiMoveNormalizedType,
): SuiMoveNormalizedType | undefined {
	return typeof normalizedType === 'object' && 'MutableReference' in normalizedType
		? normalizedType.MutableReference
		: undefined;
}

export function extractReference(
	normalizedType: SuiMoveNormalizedType,
): SuiMoveNormalizedType | undefined {
	return typeof normalizedType === 'object' && 'Reference' in normalizedType
		? normalizedType.Reference
		: undefined;
}

export function extractStructTag(
	normalizedType: SuiMoveNormalizedType,
): Extract<SuiMoveNormalizedType, { Struct: unknown }> | undefined {
	if (typeof normalizedType === 'object' && 'Struct' in normalizedType) {
		return normalizedType;
	}

	const ref = extractReference(normalizedType);
	const mutRef = extractMutableReference(normalizedType);

	if (typeof ref === 'object' && 'Struct' in ref) {
		return ref;
	}

	if (typeof mutRef === 'object' && 'Struct' in mutRef) {
		return mutRef;
	}
	return undefined;
}
