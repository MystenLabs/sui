// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionBlock, TransactionArgument } from '@mysten/sui.js/transactions';
import { getRulePackageAddress, objArg } from '../utils';
import { lock } from './kiosk';
import {
	ObjectArgument,
	RulesEnvironmentParam,
	TRANSFER_POLICY_MODULE,
	TRANSFER_POLICY_TYPE,
} from '../types';

/**
 * Call the `transfer_policy::new` function to create a new transfer policy.
 * Returns `transferPolicyCap`
 */
export function createTransferPolicy(
	tx: TransactionBlock,
	itemType: string,
	publisher: ObjectArgument,
): TransactionArgument {
	let [transferPolicy, transferPolicyCap] = tx.moveCall({
		target: `${TRANSFER_POLICY_MODULE}::new`,
		typeArguments: [itemType],
		arguments: [objArg(tx, publisher)],
	});

	tx.moveCall({
		target: `0x2::transfer::public_share_object`,
		typeArguments: [`${TRANSFER_POLICY_TYPE}<${itemType}>`],
		arguments: [transferPolicy],
	});

	return transferPolicyCap;
}

/**
 * Call the `transfer_policy::withdraw` function to withdraw profits from a transfer policy.
 */
export function withdrawFromPolicy(
	tx: TransactionBlock,
	itemType: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
	amount: string | bigint | null,
): TransactionArgument {
	let amountArg =
		amount !== null
			? tx.pure({ Some: amount }, 'Option<u64>')
			: tx.pure({ None: true }, 'Option<u64>');

	let [profits] = tx.moveCall({
		target: `${TRANSFER_POLICY_MODULE}::withdraw`,
		typeArguments: [itemType],
		arguments: [objArg(tx, policy), objArg(tx, policyCap), amountArg],
	});

	return profits;
}

/**
 * Call the `transfer_policy::confirm_request` function to unblock the
 * transaction.
 */
export function confirmRequest(
	tx: TransactionBlock,
	itemType: string,
	policy: ObjectArgument,
	request: TransactionArgument,
): void {
	tx.moveCall({
		target: `${TRANSFER_POLICY_MODULE}::confirm_request`,
		typeArguments: [itemType],
		arguments: [objArg(tx, policy), request],
	});
}

/**
 * Calls the `transfer_policy::remove_rule` function to remove a Rule from the transfer policy's ruleset.
 */
export function removeTransferPolicyRule(
	tx: TransactionBlock,
	itemType: string,
	ruleType: string,
	configType: string,
	policy: ObjectArgument,
	policyCap: TransactionArgument,
): void {
	tx.moveCall({
		target: `${TRANSFER_POLICY_MODULE}::remove_rule`,
		typeArguments: [itemType, ruleType, configType],
		arguments: [objArg(tx, policy), policyCap],
	});
}

/**
 * Calculates the amount to be paid for the royalty rule to be resolved,
 * splits the coin to pass the exact amount,
 * then calls the `royalty_rule::pay` function to resolve the royalty rule.
 */
export function resolveRoyaltyRule(
	tx: TransactionBlock,
	itemType: string,
	price: string,
	policyId: ObjectArgument,
	transferRequest: TransactionArgument,
	environment: RulesEnvironmentParam,
) {
	const policyObj = objArg(tx, policyId);
	// calculates the amount
	const [amount] = tx.moveCall({
		target: `${getRulePackageAddress(environment)}::royalty_rule::fee_amount`,
		typeArguments: [itemType],
		arguments: [policyObj, objArg(tx, price)],
	});

	// splits the coin.
	const feeCoin = tx.splitCoins(tx.gas, [amount]);

	// pays the policy
	tx.moveCall({
		target: `${getRulePackageAddress(environment)}::royalty_rule::pay`,
		typeArguments: [itemType],
		arguments: [policyObj, transferRequest, feeCoin],
	});
}

/**
 * Locks the item in the supplied kiosk and
 * proves to the `kiosk_lock` rule that the item was indeed locked,
 * by calling the `kiosk_lock_rule::prove` function to resolve it.
 */
export function resolveKioskLockRule(
	tx: TransactionBlock,
	itemType: string,
	item: TransactionArgument,
	kiosk: ObjectArgument,
	kioskCap: ObjectArgument,
	policyId: ObjectArgument,
	transferRequest: TransactionArgument,
	environment: RulesEnvironmentParam,
) {
	// lock item in the kiosk.
	lock(tx, itemType, kiosk, kioskCap, policyId, item);

	// proves that the item is locked in the kiosk to the TP.
	tx.moveCall({
		target: `${getRulePackageAddress(environment)}::kiosk_lock_rule::prove`,
		typeArguments: [itemType],
		arguments: [transferRequest, objArg(tx, kiosk)],
	});
}
