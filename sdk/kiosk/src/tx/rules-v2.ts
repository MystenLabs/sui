// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionBlock, TransactionArgument } from '@mysten/sui.js/transactions';
import { objArg } from '../utils';
import { lock } from './kiosk';
import { KioskItem, ObjectArgument } from '../types';

// The object a Rule resolving function accepts
// It can accept a set of fixed fields, that are part of every purchase flow as well any extra arguments to resolve custom policies!
// Each rule resolving function should check that the key it's seeking is in the object
// e.g. `if(!'my_key' in ruleParams!) throw new Error("Can't resolve that rule!")`
export type RuleResolvingParams = {
	tx: TransactionBlock;
	item: KioskItem; // Not sure I want this to be the type `KioskItem`. Need some extra thinking here.
	policyId: ObjectArgument;
	kiosk: ObjectArgument;
	ownedKiosk: ObjectArgument;
	ownedKioskCap: ObjectArgument;
	transferRequest: TransactionArgument;
	purchasedItem: TransactionArgument;
	packageId: string;
	extraArgs?: Record<string, ObjectArgument>; // extraParams contains more possible key,values to pass for custom rules.
};

/**
 * A helper to resolve the royalty rule.
 */
export const resolveRoyaltyRule = (params: RuleResolvingParams) => {
	const { tx, item, packageId, transferRequest, policyId } = params;

	const policyObj = objArg(tx, policyId);

	// calculates the amount
	const [amount] = tx.moveCall({
		target: `${packageId}::royalty_rule::fee_amount`,
		typeArguments: [item.type],
		arguments: [policyObj, objArg(tx, item?.listing?.price ?? '')],
	});

	// splits the coin.
	const feeCoin = tx.splitCoins(tx.gas, [amount]);

	// pays the policy
	tx.moveCall({
		target: `${packageId}::royalty_rule::pay`,
		typeArguments: [item.type],
		arguments: [policyObj, transferRequest, feeCoin],
	});
};

export const resolveKioskLockRule = (params: RuleResolvingParams) => {
	const {
		tx,
		packageId,
		item,
		ownedKiosk,
		ownedKioskCap,
		policyId,
		purchasedItem,
		transferRequest,
	} = params;

	if (!ownedKiosk || !ownedKioskCap) throw new Error('Missing Owned Kiosk or Owned Kiosk Cap');

	lock(tx, item.type, ownedKiosk, ownedKioskCap, policyId, purchasedItem);

	// proves that the item is locked in the kiosk to the TP.
	tx.moveCall({
		target: `${packageId}::kiosk_lock_rule::prove`,
		typeArguments: [item.type],
		arguments: [transferRequest, objArg(tx, ownedKiosk)],
	});
};

/**
 * A helper to resolve the personalKioskRule.
 * @param params
 */
export const resolvePersonalKioskRule = (params: RuleResolvingParams) => {
	const { tx, packageId, item, ownedKiosk, transferRequest } = params;

	if (!ownedKiosk) throw new Error('Missing owned Kiosk.');

	// proves that the destination kiosk is personal.
	tx.moveCall({
		target: `${packageId}::kiosk_lock_rule::prove`,
		typeArguments: [item.type],
		arguments: [objArg(tx, ownedKiosk), transferRequest],
	});
};

/**
 * Resolves the floor price rule.
 */
export const resolveFloorPriceRule = (params: RuleResolvingParams) => {
	const { tx, packageId, item, policyId, transferRequest } = params;

	// proves that the destination kiosk is personal
	tx.moveCall({
		target: `${packageId}::floor_price_rule::prove`,
		typeArguments: [item.type],
		arguments: [objArg(tx, policyId), transferRequest],
	});
};
