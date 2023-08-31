// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { objArg } from '../../utils';
import { lock } from '../kiosk';
import { type RuleResolvingParams } from '../../types';

/**
 * A helper to resolve the royalty rule.
 */
export function resolveRoyaltyRule(params: RuleResolvingParams) {
	const { tx, itemType, price, packageId, transferRequest, policyId } = params;

	const policyObj = objArg(tx, policyId);

	// calculates the amount
	const [amount] = tx.moveCall({
		target: `${packageId}::royalty_rule::fee_amount`,
		typeArguments: [itemType],
		arguments: [policyObj, objArg(tx, price ?? '')],
	});

	// splits the coin.
	const feeCoin = tx.splitCoins(tx.gas, [amount]);

	// pays the policy
	tx.moveCall({
		target: `${packageId}::royalty_rule::pay`,
		typeArguments: [itemType],
		arguments: [policyObj, transferRequest, feeCoin],
	});
}

export function resolveKioskLockRule(params: RuleResolvingParams) {
	const {
		tx,
		packageId,
		itemType,
		ownedKiosk,
		ownedKioskCap,
		policyId,
		purchasedItem,
		transferRequest,
	} = params;

	if (!ownedKiosk || !ownedKioskCap) throw new Error('Missing Owned Kiosk or Owned Kiosk Cap');

	lock(tx, itemType, ownedKiosk, ownedKioskCap, policyId, purchasedItem);

	// proves that the item is locked in the kiosk to the TP.
	tx.moveCall({
		target: `${packageId}::kiosk_lock_rule::prove`,
		typeArguments: [itemType],
		arguments: [transferRequest, objArg(tx, ownedKiosk)],
	});
}

/**
 * A helper to resolve the personalKioskRule.
 * @param params
 */
export function resolvePersonalKioskRule(params: RuleResolvingParams) {
	const { tx, packageId, itemType, ownedKiosk, transferRequest } = params;

	if (!ownedKiosk) throw new Error('Missing owned Kiosk.');

	// proves that the destination kiosk is personal.
	tx.moveCall({
		target: `${packageId}::kiosk_lock_rule::prove`,
		typeArguments: [itemType],
		arguments: [objArg(tx, ownedKiosk), transferRequest],
	});
}

/**
 * Resolves the floor price rule.
 */
export function resolveFloorPriceRule(params: RuleResolvingParams) {
	const { tx, packageId, itemType, policyId, transferRequest } = params;

	// proves that the destination kiosk is personal
	tx.moveCall({
		target: `${packageId}::floor_price_rule::prove`,
		typeArguments: [itemType],
		arguments: [objArg(tx, policyId), transferRequest],
	});
}
