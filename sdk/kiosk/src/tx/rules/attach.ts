// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Transaction } from '@mysten/sui/transactions';

import type { ObjectArgument } from '../../types/index.js';

export function attachKioskLockRuleTx(
	tx: Transaction,
	type: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
	packageId: string,
) {
	tx.moveCall({
		target: `${packageId}::kiosk_lock_rule::add`,
		typeArguments: [type],
		arguments: [tx.object(policy), tx.object(policyCap)],
	});
}

export function attachRoyaltyRuleTx(
	tx: Transaction,
	type: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
	percentageBps: number | string, // this is in basis points.
	minAmount: number | string,
	packageId: string,
) {
	if (Number(percentageBps) < 0 || Number(percentageBps) > 10_000)
		throw new Error('Invalid basis point percentage. Use a value between [0,10000].');

	tx.moveCall({
		target: `${packageId}::royalty_rule::add`,
		typeArguments: [type],
		arguments: [
			tx.object(policy),
			tx.object(policyCap),
			tx.pure.u16(Number(percentageBps)),
			tx.pure.u64(minAmount),
		],
	});
}

export function attachPersonalKioskRuleTx(
	tx: Transaction,
	type: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
	packageId: string,
) {
	tx.moveCall({
		target: `${packageId}::personal_kiosk_rule::add`,
		typeArguments: [type],
		arguments: [tx.object(policy), tx.object(policyCap)],
	});
}

export function attachFloorPriceRuleTx(
	tx: Transaction,
	type: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
	minPrice: string | bigint,
	packageId: string,
) {
	tx.moveCall({
		target: `${packageId}::floor_price_rule::add`,
		typeArguments: [type],
		arguments: [tx.object(policy), tx.object(policyCap), tx.pure.u64(minPrice)],
	});
}
