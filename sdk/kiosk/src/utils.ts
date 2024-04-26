// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	DynamicFieldInfo,
	PaginationArguments,
	SuiClient,
	SuiObjectData,
	SuiObjectDataFilter,
	SuiObjectDataOptions,
	SuiObjectResponse,
} from '@mysten/sui/client';
import {
	fromB64,
	normalizeStructTag,
	normalizeSuiAddress,
	parseStructTag,
} from '@mysten/sui/utils';

import { KioskType } from './bcs.js';
import type { Kiosk, KioskData, KioskListing, TransferPolicyCap } from './types/index.js';
import { TRANSFER_POLICY_CAP_TYPE } from './types/index.js';

const DEFAULT_QUERY_LIMIT = 50;

export async function getKioskObject(client: SuiClient, id: string): Promise<Kiosk> {
	const queryRes = await client.getObject({ id, options: { showBcs: true } });

	if (!queryRes || queryRes.error || !queryRes.data) {
		throw new Error(`Kiosk ${id} not found; ${queryRes.error}`);
	}

	if (!queryRes.data.bcs || !('bcsBytes' in queryRes.data.bcs)) {
		throw new Error(`Invalid kiosk query: ${id}, expected object, got package`);
	}

	return KioskType.parse(fromB64(queryRes.data.bcs!.bcsBytes));
}

// helper to extract kiosk data from dynamic fields.
export function extractKioskData(
	data: DynamicFieldInfo[],
	listings: KioskListing[],
	lockedItemIds: string[],
	kioskId: string,
): KioskData {
	return data.reduce<KioskData>(
		(acc: KioskData, val: DynamicFieldInfo) => {
			const type = val.name.type;

			if (type.startsWith('0x2::kiosk::Item')) {
				acc.itemIds.push(val.objectId);
				acc.items.push({
					objectId: val.objectId,
					type: val.objectType,
					isLocked: false,
					kioskId,
				});
			}
			if (type.startsWith('0x2::kiosk::Listing')) {
				acc.listingIds.push(val.objectId);
				listings.push({
					objectId: (val.name.value as { id: string }).id,
					listingId: val.objectId,
					isExclusive: (val.name.value as { is_exclusive: boolean }).is_exclusive,
				});
			}
			if (type.startsWith('0x2::kiosk::Lock')) {
				lockedItemIds?.push((val.name.value as { id: string }).id);
			}

			if (type.startsWith('0x2::kiosk_extension::ExtensionKey')) {
				acc.extensions.push({
					objectId: val.objectId,
					type: normalizeStructTag(parseStructTag(val.name.type).typeParams[0]),
				});
			}

			return acc;
		},
		{ items: [], itemIds: [], listingIds: [], extensions: [] },
	);
}

/**
 * A helper that attaches the listing prices to kiosk listings.
 */
export function attachListingsAndPrices(
	kioskData: KioskData,
	listings: KioskListing[],
	listingObjects: SuiObjectResponse[],
) {
	// map item listings as {item_id: KioskListing}
	// for easier mapping on the nex
	const itemListings = listings.reduce<Record<string, KioskListing>>(
		(acc: Record<string, KioskListing>, item, idx) => {
			acc[item.objectId] = { ...item };

			// return in case we don't have any listing objects.
			// that's the case when we don't have the `listingPrices` included.
			if (listingObjects.length === 0) return acc;

			const content = listingObjects[idx].data?.content;
			const data = content?.dataType === 'moveObject' ? content?.fields : null;

			if (!data) return acc;

			acc[item.objectId].price = (data as { value: string }).value;
			return acc;
		},
		{},
	);

	kioskData.items.forEach((item) => {
		item.listing = itemListings[item.objectId] || undefined;
	});
}

/**
 * A helper that attaches the listing prices to kiosk listings.
 */
export function attachObjects(kioskData: KioskData, objects: SuiObjectData[]) {
	const mapping = objects.reduce<Record<string, SuiObjectData>>(
		(acc: Record<string, SuiObjectData>, obj) => {
			acc[obj.objectId] = obj;
			return acc;
		},
		{},
	);

	kioskData.items.forEach((item) => {
		item.data = mapping[item.objectId] || undefined;
	});
}

/**
 * A Helper to attach locked state to items in Kiosk Data.
 */
export function attachLockedItems(kioskData: KioskData, lockedItemIds: string[]) {
	// map lock status in an array of type { item_id: true }
	const lockedStatuses = lockedItemIds.reduce<Record<string, boolean>>(
		(acc: Record<string, boolean>, item: string) => {
			acc[item] = true;
			return acc;
		},
		{},
	);

	// parse lockedItemIds and attach their locked status.
	kioskData.items.forEach((item) => {
		item.isLocked = lockedStatuses[item.objectId] || false;
	});
}

/**
 * A helper to fetch all DF pages.
 * We need that to fetch the kiosk DFs consistently, until we have
 * RPC calls that allow filtering of Type / batch fetching of spec
 */
export async function getAllDynamicFields(
	client: SuiClient,
	parentId: string,
	pagination: PaginationArguments<string>,
) {
	let hasNextPage = true;
	let cursor = undefined;
	const data: DynamicFieldInfo[] = [];

	while (hasNextPage) {
		const result = await client.getDynamicFields({
			parentId,
			limit: pagination.limit || undefined,
			cursor,
		});
		data.push(...result.data);
		hasNextPage = result.hasNextPage;
		cursor = result.nextCursor;
	}

	return data;
}

/**
 * A helper to fetch all objects that works with pagination.
 * It will fetch all objects in the array, and limit it to 50/request.
 * Requests are sent using `Promise.all`.
 */
export async function getAllObjects(
	client: SuiClient,
	ids: string[],
	options: SuiObjectDataOptions,
	limit: number = DEFAULT_QUERY_LIMIT,
) {
	const chunks = Array.from({ length: Math.ceil(ids.length / limit) }, (_, index) =>
		ids.slice(index * limit, index * limit + limit),
	);

	const results = await Promise.all(
		chunks.map((chunk) => {
			return client.multiGetObjects({
				ids: chunk,
				options,
			});
		}),
	);

	return results.flat();
}

/**
 * A helper to return all owned objects, with an optional filter.
 * It parses all the pages and returns the data.
 */
export async function getAllOwnedObjects({
	client,
	owner,
	filter,
	limit = DEFAULT_QUERY_LIMIT,
	options = { showType: true, showContent: true },
}: {
	client: SuiClient;
	owner: string;
	filter?: SuiObjectDataFilter;
	options?: SuiObjectDataOptions;
	limit?: number;
}) {
	let hasNextPage = true;
	let cursor = undefined;
	const data: SuiObjectResponse[] = [];

	while (hasNextPage) {
		const result = await client.getOwnedObjects({
			owner,
			filter,
			limit,
			cursor,
			options,
		});
		data.push(...result.data);
		hasNextPage = result.hasNextPage;
		cursor = result.nextCursor;
	}

	return data;
}

/**
 * Converts a number to basis points.
 * Supports up to 2 decimal points.
 * E.g 9.95 -> 995
 * @param percentage A percentage amount in the range [0, 100] including decimals.
 */
export function percentageToBasisPoints(percentage: number) {
	if (percentage < 0 || percentage > 100)
		throw new Error('Percentage needs to be in the [0,100] range.');
	return Math.ceil(percentage * 100);
}

/**
 * A helper to parse a transfer policy Cap into a usable object.
 */
export function parseTransferPolicyCapObject(
	item: SuiObjectResponse,
): TransferPolicyCap | undefined {
	const type = (item?.data?.content as { type: string })?.type;

	//@ts-ignore-next-line
	const policy = item?.data?.content?.fields?.policy_id as string;

	if (!type.includes(TRANSFER_POLICY_CAP_TYPE)) return undefined;

	// Transform 0x2::transfer_policy::TransferPolicyCap<itemType> -> itemType
	const objectType = type.replace(TRANSFER_POLICY_CAP_TYPE + '<', '').slice(0, -1);

	return {
		policyId: policy,
		policyCapId: item.data?.objectId!,
		type: objectType,
	};
}

// Normalizes the packageId part of a rule's type.
export function getNormalizedRuleType(rule: string) {
	const normalizedRuleAddress = rule.split('::');
	normalizedRuleAddress[0] = normalizeSuiAddress(normalizedRuleAddress[0]);
	return normalizedRuleAddress.join('::');
}
