// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiObjectChange } from '@mysten/sui/client';

export const getOwnerType = (change: SuiObjectChange) => {
	if (!('owner' in change)) return '';
	if (typeof change.owner === 'object') {
		if ('AddressOwner' in change.owner) return 'AddressOwner';
		if ('ObjectOwner' in change.owner) return 'ObjectOwner';
		if ('Shared' in change.owner) return 'Shared';
	}
	return change.owner;
};
