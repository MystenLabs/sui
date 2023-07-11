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
 */
export const attachRoyaltyRule = (
	tx: TransactionBlock,
	type: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
	percentage: number | string,
	min_amount: number | string,
	environment: RulesEnvironmentParam,
) => {
	tx.moveCall({
		target: `${getRulePackageAddress(environment)}::royalty_rule::add`,
		typeArguments: [type],
		arguments: [
			objArg(tx, policy),
			objArg(tx, policyCap),
			tx.pure(percentage, 'u16'),
			tx.pure(min_amount, 'u64'),
		],
	});
};

/**
 * Adds the Kiosk Lock Rule to the Transfer Policy.
 * This Rule forces buyer to lock the item in the kiosk, preserving strong royalties.
 */
export const attachKioskLockRule = (
	tx: TransactionBlock,
	type: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
	environment: RulesEnvironmentParam,
) => {
	tx.moveCall({
		target: `${getRulePackageAddress(environment)}::kiosk_lock_rule::add`,
		typeArguments: [type],
		arguments: [objArg(tx, policy), objArg(tx, policyCap)],
	});
};
