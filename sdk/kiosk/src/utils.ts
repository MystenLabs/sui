// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  JsonRpcProvider,
  ObjectId,
  SharedObjectRef,
  SuiObjectRef,
  SuiObjectResponse,
  TransactionArgument,
  TransactionBlock,
  getObjectFields,
} from '@mysten/sui.js';
import { KioskData, KioskListing } from './query/kiosk';
import { DynamicFieldInfo } from '@mysten/sui.js/dist/types/dynamic_fields';
import { bcs, Kiosk } from './bcs';

/**
 * A valid argument for any of the Kiosk functions.
 */
export type ObjectArgument =
  | string
  | TransactionArgument
  | SharedObjectRef
  | SuiObjectRef;

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

export async function getKioskObject(
  provider: JsonRpcProvider,
  id: string,
): Promise<Kiosk> {
  const queryRes = await provider.getObject({ id, options: { showBcs: true } });

  if (!queryRes || queryRes.error || !queryRes.data) {
    throw new Error(`Kiosk ${id} not found; ${queryRes.error}`);
  }

  if (!queryRes.data.bcs || !('bcsBytes' in queryRes.data.bcs)) {
    throw new Error(`Invalid kiosk query: ${id}, expected object, got package`);
  }

  return bcs.de('0x2::kiosk::Kiosk', queryRes.data.bcs!.bcsBytes, 'base64');
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
export const getTypeWithoutPackageAddress = (type: string) => {
  return type.split('::').slice(-2).join('::');
};

/**
 * A helper that attaches the listing prices to kiosk listings.
 */
export const attachListingsAndPrices = (
  kioskData: KioskData,
  listings: KioskListing[],
  listingObjects: SuiObjectResponse[],
) => {
  // map item listings as {item_id: KioskListing}
  // for easier mapping on the nex
  const itemListings = listings.reduce<Record<ObjectId, KioskListing>>(
    (acc: Record<ObjectId, KioskListing>, item, idx) => {
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

  kioskData.items.map((item) => {
    item.listing = itemListings[item.objectId] || undefined;
  });
};

/**
 * A Helper to attach locked state to items in Kiosk Data.
 */
export const attachLockedItems = (
  kioskData: KioskData,
  lockedItemIds: ObjectId[],
) => {
  // map lock status in an array of type { item_id: true }
  const lockedStatuses = lockedItemIds.reduce<Record<ObjectId, boolean>>(
    (acc: Record<ObjectId, boolean>, item: string) => {
      acc[item] = true;
      return acc;
    },
    {},
  );

  // parse lockedItemIds and attach their locked status.
  kioskData.items.map((item) => {
    item.isLocked = lockedStatuses[item.objectId] || false;
  });
};
