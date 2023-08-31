// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { objArg } from '../../utils';
import { lock } from '../kiosk';
import { type RuleResolvingParams } from '../../types';

/**
 * A helper to resolve the royalty rule.
 */
export function resolveRoyaltyRule(params: RuleResolvingParams) {
	const { txb, itemType, price, packageId, transferRequest, policyId } = params;

	const policyObj = objArg(txb, policyId);

	// calculates the amount
	const [amount] = txb.moveCall({
		target: `${packageId}::royalty_rule::fee_amount`,
		typeArguments: [itemType],
		arguments: [policyObj, objArg(txb, price ?? '')],
	});

	// splits the coin.
	const feeCoin = txb.splitCoins(txb.gas, [amount]);

	// pays the policy
	txb.moveCall({
		target: `${packageId}::royalty_rule::pay`,
		typeArguments: [itemType],
		arguments: [policyObj, transferRequest, feeCoin],
	});
}

export function resolveKioskLockRule(params: RuleResolvingParams) {
	const {
		txb,
		packageId,
		itemType,
		ownedKiosk,
		ownedKioskCap,
		policyId,
		purchasedItem,
		transferRequest,
	} = params;

	if (!ownedKiosk || !ownedKioskCap) throw new Error('Missing Owned Kiosk or Owned Kiosk Cap');

	lock(txb, itemType, ownedKiosk, ownedKioskCap, policyId, purchasedItem);

	// proves that the item is locked in the kiosk to the TP.
	txb.moveCall({
		target: `${packageId}::kiosk_lock_rule::prove`,
		typeArguments: [itemType],
		arguments: [transferRequest, objArg(txb, ownedKiosk)],
	});
}

/**
 * A helper to resolve the personalKioskRule.
 * @param params
 */
export function resolvePersonalKioskRule(params: RuleResolvingParams) {
	const { txb, packageId, itemType, ownedKiosk, transferRequest } = params;

	if (!ownedKiosk) throw new Error('Missing owned Kiosk.');

	// proves that the destination kiosk is personal.
	txb.moveCall({
		target: `${packageId}::kiosk_lock_rule::prove`,
		typeArguments: [itemType],
		arguments: [objArg(txb, ownedKiosk), transferRequest],
	});
}

/**
 * Resolves the floor price rule.
 */
export function resolveFloorPriceRule(params: RuleResolvingParams) {
	const { txb, packageId, itemType, policyId, transferRequest } = params;

	// proves that the destination kiosk is personal
	txb.moveCall({
		target: `${packageId}::floor_price_rule::prove`,
		typeArguments: [itemType],
		arguments: [objArg(txb, policyId), transferRequest],
	});
}
