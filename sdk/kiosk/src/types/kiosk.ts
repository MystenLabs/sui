// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	PaginatedObjectsResponse,
	SuiObjectData,
	SuiObjectDataOptions,
} from '@mysten/sui/client';
import type { TransactionArgument } from '@mysten/sui/transactions';

import type { ObjectArgument } from './index.js';

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
	type: string;
	/** Whether the item is Locked (there must be a `Lock` Dynamic Field) */
	isLocked: boolean;
	/** Optional listing */
	listing?: KioskListing;
	/** The ID of the kiosk the item is placed in */
	kioskId: string;
	/** Optional Kiosk Data */
	data?: SuiObjectData;
};

/** The overview type returned from `getKiosk` */
export type KioskExtensionOverview = {
	/** The ID of the extension's DF */
	objectId: string;
	/** The inner type of the Extension */
	type: string;
};
/**
 * Hold the KioskExtension data
 */
export type KioskExtension = KioskExtensionOverview & {
	/** These fields are only there if we have `withExtensions` flag */
	isEnabled: boolean;
	permissions: string;
	storageId: string;
	storageSize: number;
};

/**
 * Aggregated data from the Kiosk.
 */
export type KioskData = {
	items: KioskItem[];
	itemIds: string[];
	listingIds: string[];
	kiosk?: Kiosk;
	extensions: KioskExtensionOverview[]; // type will be defined on later versions of the SDK.
};

export type PagedKioskData = {
	data: KioskData;
	nextCursor: string | null | undefined;
	hasNextPage: boolean;
};

export type FetchKioskOptions = {
	/** Include the base kiosk object, which includes the profits, the owner and the base fields. */
	withKioskFields?: boolean;
	/** Include the listing prices. */
	withListingPrices?: boolean;
	/** Include the objects for the Items in the kiosk. Defaults to `display` only. */
	withObjects?: boolean;
	/** Pass the data options for the objects, when fetching, in case you want to query other details. */
	objectOptions?: SuiObjectDataOptions;
};

export type OwnedKiosks = {
	kioskOwnerCaps: KioskOwnerCap[];
	kioskIds: string[];
} & Omit<PaginatedObjectsResponse, 'data'>;

export type KioskOwnerCap = {
	isPersonal?: boolean;
	objectId: string;
	kioskId: string;
	digest: string;
	version: string;
};

export type PurchaseOptions = {
	extraArgs?: Record<string, any>;
};

export type ItemId = { itemType: string; itemId: string };
export type ItemReference = { itemType: string; item: ObjectArgument };
export type ItemValue = { itemType: string; item: TransactionArgument };
export type Price = { price: string | bigint };
