// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionArgument, TransactionBlock } from '@mysten/sui.js';
import { ObjectArgument, objArg } from '../utils';

/**
 * Call the `transfer_policy::new` function to create a new transfer policy.
 */
export function createTransferPolicy(
    tx: TransactionBlock,
    itemType: string,
    publisher: ObjectArgument,
): TransactionArgument {
    let [transferPolicy, transferPolicyCap] = tx.moveCall({
        target: `0x2::transfer_policy::new`,
        typeArguments: [],
        arguments: [objArg(tx, publisher)],
    });

    tx.moveCall({
        target: `0x2::transfer::public_share_object`,
        typeArguments: [itemType],
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
    let amountArg = amount !== null
        ? tx.pure([], 'vector<u64>')
        : tx.pure([amount], 'vector<u64>');

    let [profits] = tx.moveCall({
        target: `0x2::transfer_policy::withdraw`,
        typeArguments: [itemType],
        arguments: [
            objArg(tx, policy),
            objArg(tx, policyCap),
            amountArg
        ],
    });

    return profits;
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
    policyCap: TransactionArgument
): void {

    tx.moveCall({
        target: `0x2::transfer_policy::remove_rule`,
        typeArguments: [
            itemType,
            ruleType,
            configType,
        ],
        arguments: [objArg(tx, policy), policyCap],
    });

}