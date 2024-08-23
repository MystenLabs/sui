// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	PaginationArguments,
	SuiClient,
	SuiObjectData,
	SuiObjectDataFilter,
	SuiObjectResponse,
} from '@mysten/sui/client';
import { isValidSuiAddress } from '@mysten/sui/utils';

import type {
	FetchKioskOptions,
	KioskExtension,
	KioskListing,
	OwnedKiosks,
	PagedKioskData,
} from '../types/index.js';
import { KIOSK_OWNER_CAP } from '../types/index.js';
import {
	attachListingsAndPrices,
	attachLockedItems,
	attachObjects,
	extractKioskData,
	getAllDynamicFields,
	getAllObjects,
	getKioskObject,
} from '../utils.js';

export async function fetchKiosk(
	client: SuiClient,
	kioskId: string,
	pagination: PaginationArguments<string>,
	options: FetchKioskOptions,
): Promise<PagedKioskData> {
	// TODO: Replace the `getAllDynamicFields` with a paginated
	// response, once we have better RPC support for
	// type filtering & batch fetching.
	// This can't work with pagination currently.
	const data = await getAllDynamicFields(client, kioskId, pagination);

	const listings: KioskListing[] = [];
	const lockedItemIds: string[] = [];

	// extracted kiosk data.
	const kioskData = extractKioskData(data, listings, lockedItemIds, kioskId);

	// split the fetching in two queries as we are most likely passing different options for each kind.
	// For items, we usually seek the Display.
	// For listings we usually seek the DF value (price) / exclusivity.
	const [kiosk, listingObjects, items] = await Promise.all([
		options.withKioskFields ? getKioskObject(client, kioskId) : Promise.resolve(undefined),
		options.withListingPrices
			? getAllObjects(client, kioskData.listingIds, {
					showContent: true,
				})
			: Promise.resolve([]),
		options.withObjects
			? getAllObjects(client, kioskData.itemIds, options.objectOptions || { showDisplay: true })
			: Promise.resolve([]),
	]);

	if (options.withKioskFields) kioskData.kiosk = kiosk;
	// attach items listings. IF we have `options.withListingPrices === true`, it will also attach the prices.
	attachListingsAndPrices(kioskData, listings, listingObjects);
	// add `locked` status to items that are locked.
	attachLockedItems(kioskData, lockedItemIds);

	// Attach the objects for the queried items.
	attachObjects(
		kioskData,
		items.filter((x) => !!x.data).map((x) => x.data!),
	);

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
	client: SuiClient,
	address: string,
	options?: {
		pagination?: PaginationArguments<string>;
		personalKioskType: string;
	},
): Promise<OwnedKiosks> {
	if (!isValidSuiAddress(address))
		return {
			nextCursor: null,
			hasNextPage: false,
			kioskOwnerCaps: [],
			kioskIds: [],
		};

	const filter: SuiObjectDataFilter = {
		MatchAny: [
			{
				StructType: KIOSK_OWNER_CAP,
			},
		],
	};

	if (options?.personalKioskType) {
		filter.MatchAny.push({
			StructType: options.personalKioskType,
		});
	}

	// fetch owned kiosk caps, paginated.
	const { data, hasNextPage, nextCursor } = await client.getOwnedObjects({
		owner: address,
		filter,
		options: {
			showContent: true,
			showType: true,
		},
		...(options?.pagination || {}),
	});

	// get kioskIds from the OwnerCaps.
	const kioskIdList = data?.map((x: SuiObjectResponse) => {
		const fields = x.data?.content?.dataType === 'moveObject' ? x.data.content.fields : null;
		// @ts-ignore-next-line TODO: should i remove ts ignore here?
		return (fields?.cap ? fields?.cap?.fields?.for : fields?.for) as string;
		// return (fields as { for: string })?.for;
	});

	// clean up data that might have an error in them.
	// only return valid objects.
	const filteredData = data.filter((x) => 'data' in x).map((x) => x.data) as SuiObjectData[];

	return {
		nextCursor,
		hasNextPage,
		kioskOwnerCaps: filteredData.map((x, idx) => ({
			isPersonal: x.type !== KIOSK_OWNER_CAP,
			digest: x.digest,
			version: x.version,
			objectId: x.objectId,
			kioskId: kioskIdList[idx],
		})),
		kioskIds: kioskIdList,
	};
}

// Get a kiosk extension data for a given kioskId and extensionType.
export async function fetchKioskExtension(
	client: SuiClient,
	kioskId: string,
	extensionType: string,
): Promise<KioskExtension | null> {
	const extension = await client.getDynamicFieldObject({
		parentId: kioskId,
		name: {
			type: `0x2::kiosk_extension::ExtensionKey<${extensionType}>`,
			value: {
				dummy_field: false,
			},
		},
	});

	if (!extension.data) return null;

	const fields = (extension?.data?.content as { fields: { [k: string]: any } })?.fields?.value
		?.fields;

	return {
		objectId: extension.data.objectId,
		type: extensionType,
		isEnabled: fields?.is_enabled,
		permissions: fields?.permissions,
		storageId: fields?.storage?.fields?.id?.id,
		storageSize: fields?.storage?.fields?.size,
	};
}
