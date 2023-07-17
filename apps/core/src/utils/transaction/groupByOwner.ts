// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectChangesByOwner, SuiObjectChangeWithDisplay } from './getObjectChangeSummary';
import { getOwnerType } from './getOwnerType';

const getOwner = (change: SuiObjectChangeWithDisplay) => {
	// published changes don't have an owner
	if ('owner' in change && typeof change.owner === 'object') {
		if ('AddressOwner' in change.owner) return change.owner.AddressOwner;
		if ('ObjectOwner' in change.owner) return change.owner.ObjectOwner;
		if ('Shared' in change.owner) return change.objectId;
	}
	return '';
};

export const groupByOwner = (changes: SuiObjectChangeWithDisplay[]) =>
	changes.reduce((acc, change) => {
		const owner = getOwner(change);
		if (!acc[owner])
			acc[owner] = {
				changesWithDisplay: [],
				changes: [],
				ownerType: getOwnerType(change),
			};

		if (change.display?.data) {
			acc[owner].changesWithDisplay.push(change);
		} else {
			acc[owner].changes.push(change);
		}

		return acc;
	}, {} as ObjectChangesByOwner);
