// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { parseObjectType } from '../../utils/objectUtils';

import type { SuiObjectResponse, ObjectOwner } from '@mysten/sui.js/client';

export type DataType = {
	id: string;
	category?: string;
	owner: ObjectOwner;
	version: string;
	readonly?: string;
	objType: string;
	name?: string;
	ethTokenId?: string;
	publisherAddress?: string;
	contract_id?: { bytes: string };
	data: {
		contents: {
			[key: string]: any;
		};
		owner?: { ObjectOwner: [] };
		tx_digest?: string | null;
	};
	loadState?: string;
	display?: Record<string, string>;
};

/**
 * Translate the SDK response to the existing data format
 * TODO: We should redesign the rendering logic and data model
 * to make this more extensible and customizable for different Move types
 */
export function translate(o: SuiObjectResponse): DataType {
	if (o.data) {
		return {
			id: o.data.objectId,
			version: o.data.version,
			objType: parseObjectType(o),
			owner: o.data.owner!,
			data: {
				contents:
					o.data?.content?.dataType === 'moveObject'
						? o.data?.content.fields
						: o.data.content?.disassembled!,
				tx_digest: o.data.previousTransaction,
			},
			display: o.data.display?.data || undefined,
		};
	} else {
		throw new Error(`${o.error}`);
	}
}
