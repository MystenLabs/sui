// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionArgument } from '@mysten/sui.js';
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
