// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionBlock } from '@mysten/sui.js';
import { ObjectArgument, RulesEnvironmentParam } from '../types';
import { getRulePackageAddress, objArg } from '../utils';

/**
 *  Adds the Kiosk Royalty rule to the Transfer Policy.
 *  You can pass the percentage, as well as a minimum amount.
 *  The royalty that will be paid is the MAX(percentage, minAmount).
 * 	You can pass 0 in either value if you want only percentage royalty, or a fixed amount fee.
 * 	(but you should define at least one of them for the rule to make sense).
 *
 * 	@param percentageBps The royalty percentage in basis points. Use `percentageToBasisPoints` helper to convert from percentage [0,100].
 * 	@param minAmount The minimum royalty amount per request in MIST.
 */
export function attachRoyaltyRule(
	tx: TransactionBlock,
	type: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
	percentageBps: number | string, // this is in basis points.
	minAmount: number | string,
	environment: RulesEnvironmentParam,
) {
	if (Number(percentageBps) < 0 || Number(percentageBps) > 10_000)
		throw new Error('Invalid basis point percentage. Use a value between [0,10000].');

	tx.moveCall({
		target: `${getRulePackageAddress(environment)}::royalty_rule::add`,
		typeArguments: [type],
		arguments: [
			objArg(tx, policy),
			objArg(tx, policyCap),
			tx.pure(percentageBps, 'u16'),
			tx.pure(minAmount, 'u64'),
		],
	});
}

/**
 * Adds the Kiosk Lock Rule to the Transfer Policy.
 * This Rule forces buyer to lock the item in the kiosk, preserving strong royalties.
 */
export function attachKioskLockRule(
	tx: TransactionBlock,
	type: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
	environment: RulesEnvironmentParam,
) {
	tx.moveCall({
		target: `${getRulePackageAddress(environment)}::kiosk_lock_rule::add`,
		typeArguments: [type],
		arguments: [objArg(tx, policy), objArg(tx, policyCap)],
	});
}
