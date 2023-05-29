// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  JsonRpcProvider,
  PaginationArguments,
  SuiAddress,
} from '@mysten/sui.js';
import {
  attachListingsAndPrices,
  attachLockedItems,
  extractKioskData,
  getKioskObject,
} from '../utils';
import { Kiosk } from '../bcs';

/**
 * A dynamic field `Listing { ID, isExclusive }` attached to the Kiosk.
 * Holds a `u64` value - the price of the item.
 */
export type KioskListing = {
  /** The ID of the Item */
  itemId: string;
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
  itemId: string;
  /** The type of the Item */
  itemType: string;
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
  extensions: string[];
};

export type PagedKioskData = {
  data: KioskData;
  nextCursor: string | null;
  hasNextPage: boolean;
};

export type FetchKioskOptions = {
  includeKioskFields?: boolean;
  withListingPrices?: boolean;
};

export async function fetchKiosk(
  provider: JsonRpcProvider,
  kioskId: SuiAddress,
  pagination: PaginationArguments<string>,
  options: FetchKioskOptions,
): Promise<PagedKioskData> {
  const { data, nextCursor, hasNextPage } = await provider.getDynamicFields({
    parentId: kioskId,
    ...pagination,
  });

  const listings: KioskListing[] = [];
  const lockedItemIds: string[] = [];

  // extracted kiosk data.
  const kioskData = extractKioskData(data, listings, lockedItemIds);

  // split the fetching in two queries as we are most likely passing different options for each kind.
  // For items, we usually seek the Display.
  // For listings we usually seek the DF value (price) / exclusivity.
  const [kiosk, listingObjects] = await Promise.all([
    options.includeKioskFields
      ? getKioskObject(provider, kioskId)
      : Promise.resolve(undefined),
    options.withListingPrices
      ? provider.multiGetObjects({
          ids: kioskData.listingIds,
          options: {
            showBcs: true,
            showContent: true,
          },
        })
      : Promise.resolve([]),
  ]);

  if (options.includeKioskFields) kioskData.kiosk = kiosk;
  // attach items listings. IF we have `options.withListingPrices === true`, it will also attach the prices.
  attachListingsAndPrices(kioskData, listings, listingObjects);
  // add `locked` status to items that are locked.
  attachLockedItems(kioskData, lockedItemIds);

  return {
    data: kioskData,
    nextCursor,
    hasNextPage,
  };
}
