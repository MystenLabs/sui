// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	getMovePackageContent,
	getObjectId,
	getObjectVersion,
	getObjectOwner,
	getObjectFields,
	getObjectPreviousTransactionDigest,
	getObjectDisplay,
} from '@mysten/sui.js';

import { parseObjectType } from '../../utils/objectUtils';

import type { SuiObjectResponse, ObjectOwner } from '@mysten/sui.js';

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
		tx_digest?: string;
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
			id: getObjectId(o),
			version: getObjectVersion(o)!.toString(),
			objType: parseObjectType(o),
			owner: getObjectOwner(o)!,
			data: {
				contents: getObjectFields(o) ?? getMovePackageContent(o)!,
				tx_digest: getObjectPreviousTransactionDigest(o),
			},
			display: getObjectDisplay(o).data || undefined,
		};
	} else {
		throw new Error(`${o.error}`);
	}
}
