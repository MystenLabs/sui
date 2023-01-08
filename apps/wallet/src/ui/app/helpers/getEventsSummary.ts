// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
    const coinsMeta = {} as { [coinType: string]: CoinsMetaProps };
    const objectIDs: string[] = [];

    events.forEach((event) => {
        // Aggregate coin balance changes
        /// A net positive amount means the user received coins
        /// A net negative amount means the user sent coins
        if (
            'coinBalanceChange' in event &&
            ['Receive', 'Pay'].includes(event?.coinBalanceChange?.changeType)
        ) {
            const { coinBalanceChange } = event;
            const { coinType, amount, owner } = coinBalanceChange;
            const { AddressOwner } = owner as { AddressOwner: string };

            if (!coinsMeta[`${AddressOwner}${coinType}`]) {
                coinsMeta[`${AddressOwner}${coinType}`] = {
                    amount: amount,
                    coinType: coinType,
                    receiverAddress: AddressOwner,
                };
            }

            if (!coinsMeta[`${AddressOwner}${coinType}`]) {
                coinsMeta[`${AddressOwner}${coinType}`] = {
                    amount:
                        coinsMeta[`${AddressOwner}${coinType}`].amount + amount,
                    coinType: coinType,
                    receiverAddress: AddressOwner,
                };
            }
        }

        // return objectIDs of the transfer objects
        if ('transferObject' in event) {
            const { transferObject } = event;
            const { AddressOwner } = transferObject.recipient as {
                AddressOwner: string;
            };
            if (AddressOwner === address) {
                objectIDs.push(transferObject?.objectId);
            }
        }
    });

    return {
        objectIDs,
        coins: Object.values(coinsMeta),
    };
}
