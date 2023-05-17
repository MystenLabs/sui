// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    type DryRunTransactionBlockResponse,
    type SuiAddress,
    type SuiTransactionBlockResponse,
} from '@mysten/sui.js';

export type BalanceChangeSummary = {
    coinType: string;
    amount: string;
    recipient?: SuiAddress;
    owner?: SuiAddress;
};

export const getBalanceChangeSummary = (
    transaction: DryRunTransactionBlockResponse | SuiTransactionBlockResponse
) => {
    const { balanceChanges, effects } = transaction;
    if (!balanceChanges || !effects) return null;

    const negative = balanceChanges
        .filter(
            (balanceChange) =>
                typeof balanceChange.owner === 'object' &&
                'AddressOwner' in balanceChange.owner &&
                BigInt(balanceChange.amount) < 0n
        )
        .map((balanceChange) => {
            const amount = BigInt(balanceChange.amount);

            // get address owner
            const owner =
                (typeof balanceChange.owner === 'object' &&
                    'AddressOwner' in balanceChange.owner &&
                    balanceChange.owner.AddressOwner) ||
                undefined;

            // find equivalent positive balance change
            const recipient = balanceChanges.find(
                (bc) =>
                    balanceChange.coinType === bc.coinType &&
                    BigInt(balanceChange.amount) === BigInt(bc.amount) * -1n
            );

            // find recipient address
            const recipientAddress =
                (recipient &&
                    typeof recipient.owner === 'object' &&
                    'AddressOwner' in recipient.owner &&
                    recipient.owner.AddressOwner &&
                    recipient.owner.AddressOwner) ||
                undefined;

            return {
                coinType: balanceChange.coinType,
                amount: amount.toString(),
                recipient: recipientAddress,
                owner,
            };
        });

    const positive = balanceChanges
        .filter(
            (balanceChange) =>
                typeof balanceChange.owner === 'object' &&
                'AddressOwner' in balanceChange.owner &&
                BigInt(balanceChange.amount) > 0n
        )
        .map((bc) => ({
            coinType: bc.coinType,
            amount: bc.amount.toString(),
            owner:
                (typeof bc.owner === 'object' &&
                    'AddressOwner' in bc.owner &&
                    bc.owner.AddressOwner &&
                    bc.owner.AddressOwner) ||
                undefined,
        }));

    return [...positive, ...negative];
};
