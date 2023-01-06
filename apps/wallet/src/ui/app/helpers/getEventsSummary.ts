// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { notEmpty } from '_helpers';

import type { TransactionEffects } from '@mysten/sui.js';

export type CoinsMetaProps = {
    amount: number;
    coinType: string;
    receiverAddress: string;
};

export type TxnMetaResponse = {
    objectIDs: string[];
    coins: CoinsMetaProps[];
};

export function getEventsSummary(
    txEffects: TransactionEffects,
    address: string
): TxnMetaResponse {
    const events = txEffects?.events || [];

    const coinsMeta = events
        .map((event) => {
            if (
                'coinBalanceChange' in event &&
                ['Receive', 'Pay'].includes(
                    event?.coinBalanceChange?.changeType
                )
            ) {
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
    const objectIDs: string[] = events

        .map((event) => {
            if (!('transferObject' in event)) {
                return null;
            }
            const { transferObject } = event;
            const { AddressOwner } = transferObject.recipient as {
                AddressOwner: string;
            };
            if (AddressOwner !== address) {
                return null;
            }
            return transferObject?.objectId;
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

    return {
        objectIDs,
        coins: Object.values(meta),
    };
}
