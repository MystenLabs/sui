// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getMoveCallTransaction } from '@mysten/sui.js';

import { notEmpty } from '_helpers';

import type { SuiEvent, SuiTransactionKind } from '@mysten/sui.js';

// Simple wrapper to get relevant txn information from a transaction object

type CoinsMetaProps = {
    amount: number;
    coinType: string;
    receiverAddress: string;
};

export function getEventsPayReceiveSummary(
    suiEvents: SuiEvent[] | undefined
): CoinsMetaProps[] {
    const events = suiEvents || [];

    const coinsMeta = events
        .map((event) => {
            if (
                'coinBalanceChange' in event &&
                ['Receive', 'Pay'].includes(
                    event?.coinBalanceChange?.changeType
                )
            ) {
                /// Combine all the coin balance changes from Pay and Receive events
                /// A net positive amount means the user received coins
                /// A net negative amount means the user sent coins
                const { coinBalanceChange } = event;
                const { coinType, amount, coinObjectId, owner } =
                    coinBalanceChange;
                const { AddressOwner } = owner as { AddressOwner: string };
                const { ObjectOwner } = owner as { ObjectOwner: string };

                if (ObjectOwner) {
                    // TODO - update once the issue with the ObjectOwner is fixed
                    return null;
                }

                return {
                    amount: amount,
                    coinType: coinType,
                    coinObjectId: coinObjectId,
                    receiverAddress: AddressOwner,
                };
            }
            return null;
        })
        .filter(notEmpty);

    /// Group coins by receiverAddress
    // sum coins by coinType for each receiverAddress
    const meta = coinsMeta.reduce((acc, value, _) => {
        return {
            ...acc,
            [`${value.receiverAddress}${value.coinType}`]: {
                amount:
                    value.amount +
                    (acc[`${value.receiverAddress}${value.coinType}`]?.amount ||
                        0),
                coinType: value.coinType,
                receiverAddress: value.receiverAddress,
            },
        };
    }, {} as { [coinType: string]: CoinsMetaProps });

    return Object.values(meta);
}

export function getRelatedObjectIds(
    suiEvents: SuiEvent[] | undefined,
    address: string
): string[] {
    const events = suiEvents || [];
    const objectIDs = events
        ?.map((event: SuiEvent) => {
            const data = Object.values(event).find(
                (itm) => itm?.recipient?.AddressOwner === address
            );
            return data?.objectId;
        })
        .filter(notEmpty);
    return objectIDs;
}

const stakingCalls = [
    'request_add_delegation',
    'request_add_stake_with_locked_coin',
    'request_withdraw_stake',
    'request_add_delegation',
    'request_add_delegation_with_locked_coin',
    'request_withdraw_delegation',
    'request_switch_delegation',
];

// Get known native move function
export function getMoveCallMeta(txDetails: SuiTransactionKind): {
    label: string;
    fnName: string;
    validatorAddress?: string | null;
} | null {
    const moveCall = getMoveCallTransaction(txDetails);
    if (!moveCall) return null;

    let label = 'Move Call';
    let validatorAddress;
    const fnName = moveCall.function.replace(/_/g, ' ');

    if (moveCall.module === 'devnet_nft' && moveCall.function === 'mint') {
        label = 'Minted';
    }

    if (
        moveCall.module === 'sui_system' &&
        stakingCalls.includes(moveCall.function) &&
        moveCall.arguments?.[0] === '0x5'
    ) {
        // TODO properly label staking types. For now limit to Staked and Unstaked
        label =
            moveCall.function === 'request_add_delegation'
                ? 'Staked'
                : moveCall.function === 'request_withdraw_delegation'
                ? 'Unstaked!'
                : fnName;

        validatorAddress = moveCall.arguments?.[2] as string;
    }
    return {
        label,
        fnName,
        validatorAddress,
    };
}
