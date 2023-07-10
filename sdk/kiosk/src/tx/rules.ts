// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionBlock } from '@mysten/sui.js';
import { ObjectArgument, RulesEnvironmentParam } from '../types';
import { getRulePackageAddress, objArg } from '../utils';

/**
 *  Adds the Kiosk Royalty rule to the Transfer Policy.
 *  You cna provide the percentage, as well as a minimum amount.
 *  The royalty that will be paid is the MAX(percentage, minAmount).
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
