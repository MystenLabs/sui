// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiMoveNormalizedType } from '../client/index.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import type { CallArg } from './data/internal.js';

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

export function getIdFromCallArg(arg: string | CallArg) {
	if (typeof arg === 'string') {
		return normalizeSuiAddress(arg);
	}

	if (arg.Object) {
		if (arg.Object.ImmOrOwnedObject) {
			return normalizeSuiAddress(arg.Object.ImmOrOwnedObject.objectId);
		}

		if (arg.Object.Receiving) {
			return normalizeSuiAddress(arg.Object.Receiving.objectId);
		}

		return normalizeSuiAddress(arg.Object.SharedObject.objectId);
	}

	if (arg.UnresolvedObject) {
		return normalizeSuiAddress(arg.UnresolvedObject.objectId);
	}

	return undefined;
}
