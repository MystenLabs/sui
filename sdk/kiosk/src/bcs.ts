// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectOwner, bcs } from '@mysten/sui.js';

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

// Register the `Kiosk` struct for faster queries.
bcs.registerStructType('0x2::kiosk::Kiosk', {
  id: 'address',
  profits: 'u64',
  owner: 'address',
  itemCount: 'u32',
  allowExtensions: 'bool',
});

/**
 * PurchaseCap object fields (for BCS queries).
 */
export type PurchaseCap = {
  id: string;
  kioskId: string;
  itemId: string;
  minPrice: string;
};

// Register the `PurchaseCap` for faster queries.
bcs.registerStructType('0x2::kiosk::PurchaseCap', {
  id: 'address',
  kioskId: 'address',
  itemId: 'address',
  minPrice: 'u64',
});

/** Event emitted when a TransferPolicy is created. */
export type TransferPolicyCreated = {
  id: string;
};

// Register the `TransferPolicyCreated` event data.
bcs.registerStructType('0x2::transfer_policy::TransferPolicyCreated', {
  id: 'address',
});

/** The `TransferPolicy` object */
export type TransferPolicy = {
  id: string;
  type: string;
  balance: string;
  rules: string[];
  owner: ObjectOwner;
};

bcs.registerStructType('0x2::transfer_policy::TransferPolicy', {
  id: 'address',
  balance: 'u64',
  rules: ['vector', 'string'],
});

export { bcs };
