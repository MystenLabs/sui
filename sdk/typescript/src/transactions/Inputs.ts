// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SerializedBcs } from '@mysten/bcs';
import { isSerializedBcs } from '@mysten/bcs';

import { bcs } from '../bcs/index.js';
import type { ObjectCallArg, PureArg, SharedObjectRef } from '../bcs/index.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import type { CallArg, ObjectRef } from './blockData/v2.js';

function Pure(data: Uint8Array | SerializedBcs<any>, type?: string): PureArg;
/** @deprecated pass SerializedBcs values instead */
function Pure(data: unknown, type?: string): PureArg;
function Pure(data: unknown, type?: string): PureArg {
	return {
		Pure: Array.from(
			data instanceof Uint8Array
				? data
				: isSerializedBcs(data)
				? data.toBytes()
				: // NOTE: We explicitly set this to be growable to infinity, because we have maxSize validation at the builder-level:
				  bcs.ser(type!, data, { maxSize: Infinity }).toBytes(),
		),
	};
}

export const Inputs = {
	Pure,
	ObjectRef({ objectId, digest, version }: ObjectRef): ObjectCallArg {
		return {
			Object: {
				ImmOrOwnedObject: {
					digest,
					version,
					objectId: normalizeSuiAddress(objectId),
				},
			},
		};
	},
	SharedObjectRef({ objectId, mutable, initialSharedVersion }: SharedObjectRef): ObjectCallArg {
		return {
			Object: {
				SharedObject: {
					mutable,
					initialSharedVersion,
					objectId: normalizeSuiAddress(objectId),
				},
			},
		};
	},
	ReceivingRef({ objectId, digest, version }: ObjectRef): ObjectCallArg {
		return {
			Object: {
				Receiving: {
					digest,
					version,
					objectId: normalizeSuiAddress(objectId),
				},
			},
		};
	},
};

export function getIdFromCallArg(arg: string | CallArg) {
	if (typeof arg === 'string') {
		return normalizeSuiAddress(arg);
	}

	if ('Object' in arg) {
		if ('ImmOrOwnedObject' in arg.Object) {
			return normalizeSuiAddress(arg.Object.ImmOrOwnedObject.objectId);
		}

		if ('Receiving' in arg.Object) {
			return normalizeSuiAddress(arg.Object.Receiving.objectId);
		}

		return normalizeSuiAddress(arg.Object.SharedObject.objectId);
	}

	if ('UnresolvedObject' in arg) {
		return normalizeSuiAddress(arg.UnresolvedObject.value);
	}

	if ('RawValue' in arg && arg.RawValue.type === 'Object') {
		return normalizeSuiAddress(arg.RawValue.value as string);
	}

	return undefined;
}

export function getSharedObjectInput(arg: CallArg): SharedObjectRef | undefined {
	return typeof arg === 'object' && 'Object' in arg && 'SharedObject' in arg.Object
		? arg.Object.SharedObject
		: undefined;
}

export function isSharedObjectInput(arg: CallArg): boolean {
	return !!getSharedObjectInput(arg);
}

export function isMutableSharedObjectInput(arg: CallArg): boolean {
	return getSharedObjectInput(arg)?.mutable ?? false;
}
