// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { MoveStruct, SuiObjectResponse } from '../../client/index.js';
import type {
	Rpc_Move_Object_FieldsFragment,
	Rpc_Object_FieldsFragment,
} from '../generated/queries.js';
import { formatDisplay } from './display.js';
import { moveDataToRpcContent } from './move.js';
import { mapGraphQLOwnerToRpcOwner } from './owner.js';
import { toShortTypeString } from './util.js';

export function mapGraphQLObjectToRpcObject(
	object: Rpc_Object_FieldsFragment,
	options: { showBcs?: boolean | null } = {},
): NonNullable<SuiObjectResponse['data']> {
	return {
		bcs: options?.showBcs
			? {
					dataType: 'moveObject' as const,
					bcsBytes: object.asMoveObject?.contents?.bcs,
					hasPublicTransfer: object.asMoveObject?.hasPublicTransfer!,
					version: object.version as unknown as string,
					type: toShortTypeString(object.asMoveObject?.contents?.type.repr!),
			  }
			: undefined,
		content: {
			dataType: 'moveObject' as const,
			fields: moveDataToRpcContent(
				object.asMoveObject?.contents?.data!,
				object.asMoveObject?.contents?.type.layout!,
			) as MoveStruct,
			hasPublicTransfer: object.asMoveObject?.hasPublicTransfer!,
			type: toShortTypeString(object.asMoveObject?.contents?.type.repr!),
		},
		digest: object.digest!,
		display: formatDisplay(object),
		objectId: object.objectId,
		owner: mapGraphQLOwnerToRpcOwner(object.owner),
		previousTransaction: object.previousTransactionBlock?.digest,
		storageRebate: object.storageRebate,
		type: toShortTypeString(object.asMoveObject?.contents?.type.repr!),
		version: String(object.version),
	};
}

export function mapGraphQLMoveObjectToRpcObject(
	object: Rpc_Move_Object_FieldsFragment,
	options: { showBcs?: boolean | null } = {},
): NonNullable<SuiObjectResponse['data']> {
	return {
		bcs: options?.showBcs
			? {
					dataType: 'moveObject' as const,
					bcsBytes: object?.contents?.bcs,
					hasPublicTransfer: object?.hasPublicTransfer!,
					version: object.version as unknown as string,
					type: toShortTypeString(object?.contents?.type.repr!),
			  }
			: undefined,
		content: {
			dataType: 'moveObject' as const,
			fields: moveDataToRpcContent(
				object?.contents?.data!,
				object?.contents?.type.layout!,
			) as MoveStruct,
			hasPublicTransfer: object?.hasPublicTransfer!,
			type: toShortTypeString(object?.contents?.type.repr!),
		},
		digest: object.digest!,
		display: formatDisplay(object),
		objectId: object.objectId,
		owner: mapGraphQLOwnerToRpcOwner(object.owner),
		previousTransaction: object.previousTransactionBlock?.digest,
		storageRebate: object.storageRebate,
		type: toShortTypeString(object?.contents?.type.repr!),
		version: String(object.version),
	};
}
