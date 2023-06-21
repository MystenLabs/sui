// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	JsonRpcProvider,
	ObjectId,
	PaginationArguments,
	SuiAddress,
	SuiObjectData,
	SuiObjectResponse,
	getObjectFields,
	isValidSuiAddress,
} from '@mysten/sui.js';
import {
	attachListingsAndPrices,
	attachLockedItems,
	extractKioskData,
	getAllDynamicFields,
	getKioskObject,
} from '../utils';
import {
	FetchKioskOptions,
	KIOSK_OWNER_CAP,
	KioskListing,
	OwnedKiosks,
	PagedKioskData,
} from '../types';

export async function fetchKiosk(
	provider: JsonRpcProvider,
	kioskId: SuiAddress,
	pagination: PaginationArguments<string>,
	options: FetchKioskOptions,
): Promise<PagedKioskData> {
	// TODO: Replace the `getAllDynamicFields` with a paginated
	// response, once we have better RPC support for
	// type filtering & batch fetching.
	// This can't work with pagination currently.
	const data = await getAllDynamicFields(provider, kioskId, pagination);

	const listings: KioskListing[] = [];
	const lockedItemIds: ObjectId[] = [];

	// extracted kiosk data.
	const kioskData = extractKioskData(data, listings, lockedItemIds);

	// split the fetching in two queries as we are most likely passing different options for each kind.
	// For items, we usually seek the Display.
	// For listings we usually seek the DF value (price) / exclusivity.
	const [kiosk, listingObjects] = await Promise.all([
		options.withKioskFields ? getKioskObject(provider, kioskId) : Promise.resolve(undefined),
		options.withListingPrices
			? provider.multiGetObjects({
					ids: kioskData.listingIds,
					options: {
						showContent: true,
					},
			  })
			: Promise.resolve([]),
	]);

	if (options.withKioskFields) kioskData.kiosk = kiosk;
	// attach items listings. IF we have `options.withListingPrices === true`, it will also attach the prices.
	attachListingsAndPrices(kioskData, listings, listingObjects);
	// add `locked` status to items that are locked.
	attachLockedItems(kioskData, lockedItemIds);

	return {
		data: kioskData,
		nextCursor: null,
		hasNextPage: false,
	};
}

/**
 * A function to fetch all the user's kiosk Caps
 * And a list of the kiosk address ids.
 * Returns a list of `kioskOwnerCapIds` and `kioskIds`.
 * Extra options allow pagination.
 */
export async function getOwnedKiosks(
	provider: JsonRpcProvider,
	address: SuiAddress,
	options?: {
		pagination?: PaginationArguments<string>;
	},
): Promise<OwnedKiosks> {
	if (!isValidSuiAddress(address))
		return {
			nextCursor: null,
			hasNextPage: false,
			kioskOwnerCaps: [],
			kioskIds: [],
		};

	// fetch owned kiosk caps, paginated.
	const { data, hasNextPage, nextCursor } = await provider.getOwnedObjects({
		owner: address,
		filter: { StructType: KIOSK_OWNER_CAP },
		options: {
			showContent: true,
		},
		...(options?.pagination || {}),
	});

	// get kioskIds from the OwnerCaps.
	const kioskIdList = data?.map((x: SuiObjectResponse) => getObjectFields(x)?.for);

	// clean up data that might have an error in them.
	// only return valid objects.
	const filteredData = data.filter((x) => 'data' in x).map((x) => x.data) as SuiObjectData[];

	return {
		nextCursor,
		hasNextPage,
		kioskOwnerCaps: filteredData.map((x, idx) => ({
			digest: x.digest,
			version: x.version,
			objectId: x.objectId,
			kioskId: kioskIdList[idx],
		})),
		kioskIds: kioskIdList,
	};
}
