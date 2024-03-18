// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SerializedBcs } from '@mysten/bcs';

import type { SharedObjectRef } from '../bcs/index.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import type { CallArg, ObjectRef } from './blockData/v2.js';

function Pure(data: Uint8Array | SerializedBcs<any>): Extract<CallArg, { Pure: unknown }> {
	return {
		$kind: 'Pure',
		Pure: Array.from(data instanceof Uint8Array ? data : data.toBytes()),
	};
}

export const Inputs = {
	Pure,
	ObjectRef({ objectId, digest, version }: ObjectRef): Extract<CallArg, { Object: unknown }> {
		return {
			$kind: 'Object',
			Object: {
				$kind: 'ImmOrOwnedObject',
				ImmOrOwnedObject: {
					digest,
					version,
					objectId: normalizeSuiAddress(objectId),
				},
			},
		};
	},
	SharedObjectRef({
		objectId,
		mutable,
		initialSharedVersion,
	}: SharedObjectRef): Extract<CallArg, { Object: unknown }> {
		return {
			$kind: 'Object',
			Object: {
				$kind: 'SharedObject',
				SharedObject: {
					mutable,
					initialSharedVersion,
					objectId: normalizeSuiAddress(objectId),
				},
			},
		};
	},
	ReceivingRef({ objectId, digest, version }: ObjectRef): Extract<CallArg, { Object: unknown }> {
		return {
			$kind: 'Object',
			Object: {
				$kind: 'Receiving',
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
		return normalizeSuiAddress(arg.UnresolvedObject.value);
	}

	return undefined;
}

export function getSharedObjectInput(arg: CallArg): SharedObjectRef | undefined {
	return typeof arg === 'object' && arg.Object?.SharedObject ? arg.Object.SharedObject : undefined;
}

export function isSharedObjectInput(arg: CallArg): boolean {
	return !!getSharedObjectInput(arg);
}

export function isMutableSharedObjectInput(arg: CallArg): boolean {
	return getSharedObjectInput(arg)?.mutable ?? false;
}
