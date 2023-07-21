// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectDigest, ObjectType, PaginatedObjectsResponse } from '@mysten/sui.js';
import { TransactionArgument } from '@mysten/sui.js/transactions';
import { ObjectArgument } from '.';

/** The Kiosk module. */
export const KIOSK_MODULE = '0x2::kiosk';

/** The Kiosk type. */
export const KIOSK_TYPE = `${KIOSK_MODULE}::Kiosk`;

/** The Kiosk Owner Cap Type */
export const KIOSK_OWNER_CAP = `${KIOSK_MODULE}::KioskOwnerCap`;

/** The Kiosk Item Type */
export const KIOSK_ITEM = `${KIOSK_MODULE}::Item`;

/** The Kiosk Listing Type */
export const KIOSK_LISTING = `${KIOSK_MODULE}::Listing`;

/** The Kiosk Lock Type */
export const KIOSK_LOCK = `${KIOSK_MODULE}::Lock`;

/** The Kiosk PurchaseCap type */
export const KIOSK_PURCHASE_CAP = `${KIOSK_MODULE}::PurchaseCap`;

/**
 * The Kiosk object fields (for BCS queries).
 */
export type Kiosk = {
	id: string;
	profits: string;
	owner: string;
	itemCount: number;
	allowExtensions: boolean;
};

/**
 * PurchaseCap object fields (for BCS queries).
 */
export type PurchaseCap = {
	id: string;
	kioskId: string;
	itemId: string;
	minPrice: string;
};

/**
 * The response type of a successful purchase flow.
 * Returns the item, and a `canTransfer` param.
 */
export type PurchaseAndResolvePoliciesResponse = {
	item: TransactionArgument;
	canTransfer: boolean;
};

/**
 * Optional parameters for `purchaseAndResolvePolicies` flow.
 * This gives us the chance to extend the function in further releases
 * without introducing more breaking changes.
 */
export type PurchaseOptionalParams = {
	ownedKiosk?: ObjectArgument;
	ownedKioskCap?: ObjectArgument;
};

/**
 * A dynamic field `Listing { ID, isExclusive }` attached to the Kiosk.
 * Holds a `u64` value - the price of the item.
 */
export type KioskListing = {
	/** The ID of the Item */
	objectId: string;
	/**
	 * Whether or not there's a `PurchaseCap` issued. `true` means that
	 * the listing is controlled by some logic and can't be purchased directly.
	 *
	 * TODO: consider renaming the field for better indication.
	 */
	isExclusive: boolean;
	/** The ID of the listing */
	listingId: string;
	price?: string;
};

/**
 * A dynamic field `Item { ID }` attached to the Kiosk.
 * Holds an Item `T`. The type of the item is known upfront.
 */
export type KioskItem = {
	/** The ID of the Item */
	objectId: string;
	/** The type of the Item */
	type: ObjectType;
	/** Whether the item is Locked (there must be a `Lock` Dynamic Field) */
	isLocked: boolean;
	/** Optional listing */
	listing?: KioskListing;
};
/**
 * Aggregated data from the Kiosk.
 */
export type KioskData = {
	items: KioskItem[];
	itemIds: string[];
	listingIds: string[];
	kiosk?: Kiosk;
	extensions: any[]; // type will be defined on later versions of the SDK.
};

export type PagedKioskData = {
	data: KioskData;
	nextCursor: string | null;
	hasNextPage: boolean;
};

export type FetchKioskOptions = {
	withKioskFields?: boolean;
	withListingPrices?: boolean;
};

export type OwnedKiosks = {
	kioskOwnerCaps: KioskOwnerCap[];
	kioskIds: string[];
} & Omit<PaginatedObjectsResponse, 'data'>;

export type KioskOwnerCap = {
	objectId: string;
	kioskId: string;
	digest: ObjectDigest;
	version: string;
};
