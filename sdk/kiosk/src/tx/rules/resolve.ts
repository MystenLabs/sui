// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { RuleResolvingParams } from '../../types/index.js';
import { lock } from '../kiosk.js';

/**
 * A helper to resolve the royalty rule.
 */
export function resolveRoyaltyRule(params: RuleResolvingParams) {
	const { transaction: tx, itemType, price, packageId, transferRequest, policyId } = params;

	const policyObj = tx.object(policyId);

	// calculates the amount
	const [amount] = tx.moveCall({
		target: `${packageId}::royalty_rule::fee_amount`,
		typeArguments: [itemType],
		arguments: [policyObj, tx.pure.u64(price || '0')],
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
		transaction: tx,
		packageId,
		itemType,
		kiosk,
		kioskCap,
		policyId,
		purchasedItem,
		transferRequest,
	} = params;

	if (!kiosk || !kioskCap) throw new Error('Missing Owned Kiosk or Owned Kiosk Cap');

	lock(tx, itemType, kiosk, kioskCap, policyId, purchasedItem);

	// proves that the item is locked in the kiosk to the TP.
	tx.moveCall({
		target: `${packageId}::kiosk_lock_rule::prove`,
		typeArguments: [itemType],
		arguments: [transferRequest, tx.object(kiosk)],
	});
}

/**
 * A helper to resolve the personalKioskRule.
 * @param params
 */
export function resolvePersonalKioskRule(params: RuleResolvingParams) {
	const { transaction: tx, packageId, itemType, kiosk, transferRequest } = params;

	if (!kiosk) throw new Error('Missing owned Kiosk.');

	// proves that the destination kiosk is personal.
	tx.moveCall({
		target: `${packageId}::personal_kiosk_rule::prove`,
		typeArguments: [itemType],
		arguments: [tx.object(kiosk), transferRequest],
	});
}

/**
 * Resolves the floor price rule.
 */
export function resolveFloorPriceRule(params: RuleResolvingParams) {
	const { transaction: tx, packageId, itemType, policyId, transferRequest } = params;

	// proves that the destination kiosk is personal
	tx.moveCall({
		target: `${packageId}::floor_price_rule::prove`,
		typeArguments: [itemType],
		arguments: [tx.object(policyId), transferRequest],
	});
}
