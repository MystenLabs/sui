// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui/bcs';
import type { TransactionArgument } from '@mysten/sui/transactions';
import { Transaction } from '@mysten/sui/transactions';
import { normalizeSuiAddress } from '@mysten/sui/utils';

import type { RuleResolvingParams } from '../../types/index.js';
import { lock } from '../kiosk.js';

/**
 * A helper to resolve the royalty rule.
 */
export async function resolveRoyaltyRule(params: RuleResolvingParams) {
	const {
		kioskClient,
		transaction: tx,
		itemType,
		price,
		packageId,
		transferRequest,
		policyId,
	} = params;

	const policyObj = tx.object(policyId);

	// We attempt to resolve the fee amount outside of the PTB so that the split amount is known before the transaction is sent.
	// This improves the display of the transaction within the wallet.

	const feeTx = new Transaction();
	// calculates the amount
	feeTx.moveCall({
		target: `${packageId}::royalty_rule::fee_amount`,
		typeArguments: [itemType],
		arguments: [policyObj, tx.pure.u64(price || '0')],
	});

	const { results } = await kioskClient.client.devInspectTransactionBlock({
		sender: tx.getData().sender || normalizeSuiAddress('0x0'),
		transactionBlock: feeTx,
	});

	let amount: TransactionArgument | bigint | null = null;
	if (results) {
		const returnedAmount = results?.[0].returnValues?.[0]?.[0];
		if (returnedAmount) {
			amount = BigInt(bcs.U64.parse(new Uint8Array(returnedAmount as number[])));
		}
	}

	// We were not able to calculate the amount outside of the transaction, so fall back to resolving it within the PTB
	if (!amount) {
		[amount] = tx.moveCall({
			target: `${packageId}::royalty_rule::fee_amount`,
			typeArguments: [itemType],
			arguments: [policyObj, tx.pure.u64(price || '0')],
		});
	}

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
