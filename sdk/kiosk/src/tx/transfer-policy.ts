// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui/bcs';
import type {
	Transaction,
	TransactionArgument,
	TransactionObjectArgument,
} from '@mysten/sui/transactions';

import type { ObjectArgument } from '../types/index.js';
import { TRANSFER_POLICY_MODULE, TRANSFER_POLICY_TYPE } from '../types/index.js';

/**
 * Call the `transfer_policy::new` function to create a new transfer policy.
 * Returns `transferPolicyCap`
 */
export function createTransferPolicy(
	tx: Transaction,
	itemType: string,
	publisher: ObjectArgument,
): TransactionObjectArgument {
	const [transferPolicy, transferPolicyCap] = createTransferPolicyWithoutSharing(
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
	tx: Transaction,
	itemType: string,
	publisher: ObjectArgument,
): [TransactionObjectArgument, TransactionObjectArgument] {
	const [transferPolicy, transferPolicyCap] = tx.moveCall({
		target: `${TRANSFER_POLICY_MODULE}::new`,
		typeArguments: [itemType],
		arguments: [tx.object(publisher)],
	});

	return [transferPolicy, transferPolicyCap];
}
/**
 * Converts Transfer Policy to a shared object.
 */
export function shareTransferPolicy(
	tx: Transaction,
	itemType: string,
	transferPolicy: TransactionObjectArgument,
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
	tx: Transaction,
	itemType: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
	amount?: string | bigint | null,
): TransactionObjectArgument {
	const amountArg = bcs.option(bcs.u64()).serialize(amount);

	const [profits] = tx.moveCall({
		target: `${TRANSFER_POLICY_MODULE}::withdraw`,
		typeArguments: [itemType],
		arguments: [tx.object(policy), tx.object(policyCap), amountArg],
	});

	return profits;
}

/**
 * Call the `transfer_policy::confirm_request` function to unblock the
 * transaction.
 */
export function confirmRequest(
	tx: Transaction,
	itemType: string,
	policy: ObjectArgument,
	request: TransactionArgument,
): void {
	tx.moveCall({
		target: `${TRANSFER_POLICY_MODULE}::confirm_request`,
		typeArguments: [itemType],
		arguments: [tx.object(policy), request],
	});
}

/**
 * Calls the `transfer_policy::remove_rule` function to remove a Rule from the transfer policy's ruleset.
 */
export function removeTransferPolicyRule(
	tx: Transaction,
	itemType: string,
	ruleType: string,
	configType: string,
	policy: ObjectArgument,
	policyCap: ObjectArgument,
): void {
	tx.moveCall({
		target: `${TRANSFER_POLICY_MODULE}::remove_rule`,
		typeArguments: [itemType, ruleType, configType],
		arguments: [tx.object(policy), tx.object(policyCap)],
	});
}
