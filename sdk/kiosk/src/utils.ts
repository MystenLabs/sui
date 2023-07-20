// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SharedObjectRef, SuiObjectRef, SuiObjectResponse, getObjectFields } from '@mysten/sui.js';
import { TransactionBlock, TransactionArgument } from '@mysten/sui.js/transactions';
import { type DynamicFieldInfo } from '@mysten/sui.js';
import { bcs } from './bcs';
import { KIOSK_TYPE, Kiosk, KioskData, KioskListing, RulesEnvironmentParam } from './types';
import { MAINNET_RULES_PACKAGE_ADDRESS, TESTNET_RULES_PACKAGE_ADDRESS } from './constants';
import { SuiClient, PaginationArguments } from '@mysten/sui.js/client';

/* A simple map to the rule package addresses */
// TODO: Supply the mainnet and devnet addresses.
export const rulesPackageAddresses = {
	mainnet: MAINNET_RULES_PACKAGE_ADDRESS,
	testnet: TESTNET_RULES_PACKAGE_ADDRESS,
	devnet: '',
	custom: null,
};

/**
 * Convert any valid input into a TransactionArgument.
 *
 * @param tx The transaction to use for creating the argument.
 * @param arg The argument to convert.
 * @returns The converted TransactionArgument.
 */
export function objArg(
	tx: TransactionBlock,
	arg: string | SharedObjectRef | SuiObjectRef | TransactionArgument,
): TransactionArgument {
	if (typeof arg === 'string') {
		return tx.object(arg);
	}

	if ('digest' in arg && 'version' in arg && 'objectId' in arg) {
		return tx.objectRef(arg);
	}

	if ('objectId' in arg && 'initialSharedVersion' in arg && 'mutable' in arg) {
		return tx.sharedObjectRef(arg);
	}

	if ('kind' in arg) {
		return arg;
	}

	throw new Error('Invalid argument type');
}

export async function getKioskObject(client: SuiClient, id: string): Promise<Kiosk> {
	const queryRes = await client.getObject({ id, options: { showBcs: true } });

	if (!queryRes || queryRes.error || !queryRes.data) {
		throw new Error(`Kiosk ${id} not found; ${queryRes.error}`);
	}

	if (!queryRes.data.bcs || !('bcsBytes' in queryRes.data.bcs)) {
		throw new Error(`Invalid kiosk query: ${id}, expected object, got package`);
	}

	return bcs.de(KIOSK_TYPE, queryRes.data.bcs!.bcsBytes, 'base64');
}

// helper to extract kiosk data from dynamic fields.
export function extractKioskData(
	data: DynamicFieldInfo[],
	listings: KioskListing[],
	lockedItemIds: string[],
): KioskData {
	return data.reduce<KioskData>(
		(acc: KioskData, val: DynamicFieldInfo) => {
			const type = getTypeWithoutPackageAddress(val.name.type);

			switch (type) {
				case 'kiosk::Item':
					acc.itemIds.push(val.objectId);
					acc.items.push({
						objectId: val.objectId,
						type: val.objectType,
						isLocked: false,
					});
					break;
				case 'kiosk::Listing':
					acc.listingIds.push(val.objectId);
					listings.push({
						objectId: val.name.value.id,
						listingId: val.objectId,
						isExclusive: val.name.value.is_exclusive,
					});
					break;
				case 'kiosk::Lock':
					lockedItemIds?.push(val.name.value.id);
					break;
			}
			return acc;
		},
		{ items: [], itemIds: [], listingIds: [], extensions: [] },
	);
}

// e.g. 0x2::kiosk::Item -> kiosk::Item
export function getTypeWithoutPackageAddress(type: string) {
	return type.split('::').slice(-2).join('::');
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

			const data = getObjectFields(listingObjects[idx]);
			if (!data) return acc;

			acc[item.objectId].price = data.value;
			return acc;
		},
		{},
	);

	kioskData.items.forEach((item) => {
		item.listing = itemListings[item.objectId] || undefined;
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
 * A helper to get a rule's environment address.
 */
export function getRulePackageAddress(environment: RulesEnvironmentParam): string {
	// if we have custom environment, we return it.
	if (environment.env === 'custom') {
		if (!environment.address)
			throw new Error('Please supply the custom package address for rules.');
		return environment.address;
	}
	return rulesPackageAddresses[environment.env];
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
