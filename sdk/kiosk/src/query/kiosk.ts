// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  JsonRpcProvider,
  PaginationArguments,
  SuiAddress,
} from '@mysten/sui.js';

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
  /** Can be used to query a dynamic field */
  bcsName: string;
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
  /** Can be used to query a dynamic field */
  bcsName: string;
};

/**
 * Aggregated data from the Kiosk.
 */
export type KioskData = {
  items: KioskItem[];
  listings: KioskListing[];
  itemIds: string[];
  listingIds: string[];
};

export type PagedKioskData = {
  data: KioskData;
  nextCursor: string | null;
  hasNextPage: boolean;
};

export async function fetchKiosk(
  provider: JsonRpcProvider,
  kioskId: SuiAddress,
  pagination: PaginationArguments<string>,
): Promise<PagedKioskData> {
  const { data, nextCursor, hasNextPage } = await provider.getDynamicFields({
    parentId: kioskId,
    ...pagination,
  });

  const kioskData = data.reduce<KioskData>(
    (acc, val) => {
      // e.g. 0x2::kiosk::Item -> kiosk::Item
      const type = val.name.type.split('::').slice(-2).join('::');

      switch (type) {
        case 'kiosk::Item':
          acc.itemIds.push(val.objectId);
          acc.items.push({
            itemId: val.objectId,
            itemType: val.objectType,
            bcsName: val.bcsName,
          });
          break;
        case 'kiosk::Listing':
          acc.listingIds.push(val.objectId);
          acc.listings.push({
            itemId: val.name.value.id,
            listingId: val.objectId,
            isExclusive: val.name.value.is_exclusive,
            bcsName: val.bcsName,
          });
          break;
      }
      return acc;
    },
    { listings: [], items: [], itemIds: [], listingIds: [] },
  );

  return {
    data: kioskData,
    nextCursor,
    hasNextPage,
  };
}
