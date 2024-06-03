// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ObjectOwner } from '@mysten/sui/client';
import type { Transaction, TransactionObjectArgument } from '@mysten/sui/transactions';

import type { ObjectArgument } from './index.js';

/** The Transfer Policy module. */
export const TRANSFER_POLICY_MODULE = '0x2::transfer_policy';

/** Name of the event emitted when a TransferPolicy for T is created. */
export const TRANSFER_POLICY_CREATED_EVENT = `${TRANSFER_POLICY_MODULE}::TransferPolicyCreated`;

/** The Transfer Policy Type */
export const TRANSFER_POLICY_TYPE = `${TRANSFER_POLICY_MODULE}::TransferPolicy`;

/** The Transfer Policy Cap Type */
export const TRANSFER_POLICY_CAP_TYPE = `${TRANSFER_POLICY_MODULE}::TransferPolicyCap`;

/** The Kiosk Lock Rule */
export const KIOSK_LOCK_RULE = 'kiosk_lock_rule::Rule';

/** The Royalty rule */
export const ROYALTY_RULE = 'royalty_rule::Rule';

/**
 * The Transfer Policy Cap in a consumable way.
 */
export type TransferPolicyCap = {
	policyId: string;
	policyCapId: string;
	type: string;
};

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

// The object a Rule resolving function accepts
// It can accept a set of fixed fields, that are part of every purchase flow as well any extra arguments to resolve custom policies!
// Each rule resolving function should check that the key it's seeking is in the object
// e.g. `if(!'my_key' in ruleParams!) throw new Error("Can't resolve that rule!")`
export type RuleResolvingParams = {
	transaction: Transaction;
	/** @deprecated use transaction instead */
	transactionBlock: Transaction;
	itemType: string;
	itemId: string;
	price: string;
	policyId: ObjectArgument;
	sellerKiosk: ObjectArgument;
	kiosk: ObjectArgument;
	kioskCap: ObjectArgument;
	transferRequest: TransactionObjectArgument;
	purchasedItem: TransactionObjectArgument;
	packageId: string;
	extraArgs: Record<string, any>; // extraParams contains more possible {key, values} to pass for custom rules.
};
