// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { DisplayFieldsResponse, ObjectId, SuiObjectResponse } from '@mysten/sui.js';
import { hasDisplayData } from '../hasDisplayData';

export function getObjectDisplayLookup(objects: SuiObjectResponse[] = []) {
	const lookup: Map<ObjectId, DisplayFieldsResponse> = new Map();
	return objects?.filter(hasDisplayData).reduce((acc, curr) => {
		if (curr.data?.objectId) {
			acc.set(curr.data.objectId, curr.data.display as DisplayFieldsResponse);
		}
		return acc;
	}, lookup);
}
