// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui/bcs';

import {
	KIOSK_PURCHASE_CAP,
	KIOSK_TYPE,
	TRANSFER_POLICY_CREATED_EVENT,
	TRANSFER_POLICY_TYPE,
} from './types/index.js';

// Register the `Kiosk` struct for faster queries.
export const KioskType = bcs.struct(KIOSK_TYPE, {
	id: bcs.Address,
	profits: bcs.u64(),
	owner: bcs.Address,
	itemCount: bcs.u32(),
	allowExtensions: bcs.bool(),
});

// Register the `PurchaseCap` for faster queries.
export const KioskPurchaseCap = bcs.struct(KIOSK_PURCHASE_CAP, {
	id: bcs.Address,
	kioskId: bcs.Address,
	itemId: bcs.Address,
	minPrice: bcs.u64(),
});

// Register the `TransferPolicyCreated` event data.
export const TransferPolicyCreatedEvent = bcs.struct(TRANSFER_POLICY_CREATED_EVENT, {
	id: bcs.Address,
});

export const TransferPolicyType = bcs.struct(TRANSFER_POLICY_TYPE, {
	id: bcs.Address,
	balance: bcs.u64(),
	rules: bcs.vector(bcs.string()),
});
