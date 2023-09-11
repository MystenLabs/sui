// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionBlock, TransactionArgument } from '@mysten/sui.js/transactions';
import { objArg } from '../utils';
import { ObjectArgument, TRANSFER_POLICY_MODULE, TRANSFER_POLICY_TYPE } from '../types';

/**
 * Call the `transfer_policy::new` function to create a new transfer policy.
 * Returns `transferPolicyCap`
 */
export function createTransferPolicy(
	tx: TransactionBlock,
	itemType: string,
	publisher: ObjectArgument,
): TransactionArgument {
	let [transferPolicy, transferPolicyCap] = createTransferPolicyWithoutSharing(
		tx,
		itemType,
		publisher,
	);

	shareTransferPolicy(tx, itemType, transferPolicy);

	return transferPolicyCap;
}

/**
 * Creates a transfer Policy and returns both the Policy and the Cap.
 * Used if we want to use the policy before making it a shared object.
 */
export function createTransferPolicyWithoutSharing(
	tx: TransactionBlock,
	itemType: string,
	publisher: ObjectArgument,
): [TransactionArgument, TransactionArgument] {
	let [transferPolicy, transferPolicyCap] = tx.moveCall({
		target: `${TRANSFER_POLICY_MODULE}::new`,
		typeArguments: [itemType],
		arguments: [objArg(tx, publisher)],
	});

	return [transferPolicy, transferPolicyCap];
}
/**
 * Converts Transfer Policy to a shared object.
 */
export function shareTransferPolicy(
	tx: TransactionBlock,
	itemType: string,
	transferPolicy: TransactionArgument,
) {
	tx.moveCall({
		target: `0x2::transfer::public_share_object`,
		typeArguments: [`${TRANSFER_POLICY_TYPE}<${itemType}>`],
		arguments: [transferPolicy],
	});
}

/**
 * Call the `transfer_policy::withdraw` function to withdraw profits from a transfer policy.
 */
export function withdrawFromPolicy(
	tx: TransactionBlock,
	itemType: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
	amount?: string | bigint | null,
): TransactionArgument {
	const amountArg = tx.pure(amount ? { Some: amount } : { None: true }, 'Option<u64>');

	const [profits] = tx.moveCall({
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
	policyCap: ObjectArgument,
): void {
	tx.moveCall({
		target: `${TRANSFER_POLICY_MODULE}::remove_rule`,
		typeArguments: [itemType, ruleType, configType],
		arguments: [objArg(tx, policy), objArg(tx, policyCap)],
	});
}
