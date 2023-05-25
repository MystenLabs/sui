// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  JsonRpcProvider,
  PaginationArguments,
  SuiAddress,
  SuiObjectDataOptions,
  SuiObjectResponse,
  getObjectFields,
} from '@mysten/sui.js';
import { extractKioskData, getKioskObject, getObjects } from '../utils';
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
};

/**
 * Aggregated data from the Kiosk.
 */
export type KioskData = {
  items: KioskItem[] | SuiObjectResponse[];
  listings: KioskListing[];
  itemIds: string[];
  listingIds: string[];
  kiosk?: Kiosk;
};

export type PagedKioskData = {
  data: KioskData;
  nextCursor: string | null;
  hasNextPage: boolean;
};

export type FetchKioskOptions = {
  includeKioskFields?: boolean;
  includeItems?: boolean;
  itemOptions?: SuiObjectDataOptions;
  withListingPrices?: boolean;
  listingOptions?: SuiObjectDataOptions;
};

/**
 *
 */
export async function fetchKiosk(
  provider: JsonRpcProvider,
  kioskId: SuiAddress,
  pagination: PaginationArguments<string>,
  {
    includeKioskFields = false,
    includeItems = false,
    withListingPrices = false,
    itemOptions = { showDisplay: true, showType: true },
  }: FetchKioskOptions,
): Promise<PagedKioskData> {
  const { data, nextCursor, hasNextPage } = await provider.getDynamicFields({
    parentId: kioskId,
    ...pagination,
  });

  // extracted kiosk data.
  const kioskData = extractKioskData(data);

  // split the fetching in two queries as we are most likely passing different options for each kind.
  // For items, we usually seek the Display.
  // For listings we usually seek the DF value (price) / exclusivity.
  const [kiosk, itemObjects, listingObjects] = await Promise.all([
    includeKioskFields
      ? getKioskObject(provider, kioskId)
      : Promise.resolve(undefined),
    includeItems
      ? getObjects(provider, kioskData.itemIds, itemOptions)
      : Promise.resolve([]),
    withListingPrices
      ? getObjects(provider, kioskData.listingIds, {
          showBcs: true,
          showContent: true,
        })
      : Promise.resolve([]),
  ]);

  if (includeKioskFields) kioskData.kiosk = kiosk;
  if (includeItems) kioskData.items = itemObjects;
  if (withListingPrices)
    kioskData.listings.map((l, i) => {
      const fields = getObjectFields(listingObjects[i]);
      // l.price = bcs.de('u64', listingObjects[i].data?.bcs.bcsBytes, 'base64');
      // TODO: Figure out a way to do this with BCS to avoid querying content.
      l.price = fields?.value;
      return l;
    });

  return {
    data: kioskData,
    nextCursor,
    hasNextPage,
  };
}
