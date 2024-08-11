// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { KioskListing, KioskOwnerCap } from '@mysten/kiosk';
import { SuiObjectResponse } from '@mysten/sui/client';
import { MIST_PER_SUI, normalizeSuiAddress } from '@mysten/sui/utils';

// Parse the display of a list of objects into a simple {object_id: display} map
// to use throughout the app.
export const parseObjectDisplays = (
	data: SuiObjectResponse[],
): Record<string, Record<string, string> | undefined> => {
	return data.reduce<Record<string, Record<string, string> | undefined>>(
		(acc, item: SuiObjectResponse) => {
			const display = item.data?.display?.data;
			const id = item.data?.objectId!;
			acc[id] = display || undefined;
			return acc;
		},
		{},
	);
};

export const processKioskListings = (data: KioskListing[]): Record<string, KioskListing> => {
	const results: Record<string, KioskListing> = {};

	data
		.filter((x) => !!x)
		.map((x: KioskListing) => {
			results[x.objectId || ''] = x;
			return x;
		});
	return results;
};

export const mistToSui = (mist: bigint | string | undefined) => {
	if (!mist) return 0;
	return Number(mist || 0) / Number(MIST_PER_SUI);
};

export const formatSui = (amount: number) => {
	return new Intl.NumberFormat('en-US', {
		minimumFractionDigits: 2,
		maximumFractionDigits: 5,
	}).format(amount);
};

/**
 * Finds an active owner cap for a kioskId based on the
 * address owned kiosks.
 */
export const findActiveCap = (
	caps: KioskOwnerCap[] = [],
	kioskId: string,
): KioskOwnerCap | undefined => {
	return caps.find((x) => normalizeSuiAddress(x.kioskId) === normalizeSuiAddress(kioskId));
};
