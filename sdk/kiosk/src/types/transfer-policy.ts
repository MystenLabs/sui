// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectOwner } from '@mysten/sui.js';

/** The Transfer Policy module. */
export const TRANSFER_POLICY_MODULE = '0x2::transfer_policy';

/** Name of the event emitted when a TransferPolicy for T is created. */
export const TRANSFER_POLICY_CREATED_EVENT = `${TRANSFER_POLICY_MODULE}::TransferPolicyCreated`;

/** The Transfer Policy Type */
export const TRANSFER_POLICY_TYPE = `${TRANSFER_POLICY_MODULE}::TransferPolicy`;

/** The Kiosk Lock Rule */
export const KIOSK_LOCK_RULE = 'kiosk_lock_rule::Rule';

/** The Royalty rule */
export const ROYALTY_RULE = 'royalty_rule::Rule';

/** The `TransferPolicy` object */
export type TransferPolicy = {
	id: string;
	type: string;
	balance: string;
	rules: string[];
	owner: ObjectOwner;
};

/** Event emitted when a TransferPolicy is created. */
export type TransferPolicyCreated = {
	id: string;
};
