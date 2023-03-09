// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { TransactionEvents } from '@mysten/sui.js';

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
    events: TransactionEvents,
    address: string
): TxnMetaResponse {
    const coinsMeta = {} as { [coinType: string]: CoinsMetaProps };
    const objectIDs: string[] = [];

    events.forEach((event) => {
        // Aggregate coinBalanceChange by coinType and address
        // A net positive amount means the user received coins
        // A net negative amount means the user sent coins
        if (
            'coinBalanceChange' in event &&
            event?.coinBalanceChange?.changeType &&
            ['Receive', 'Pay'].includes(event?.coinBalanceChange?.changeType) &&
            event?.coinBalanceChange?.transactionModule !== 'gas'
        ) {
            const { coinBalanceChange } = event;
            const { coinType, amount, owner, sender } = coinBalanceChange;
            const { AddressOwner } = owner as { AddressOwner: string };

            // ChangeEpoch txn includes coinBalanceChange event for other addresses
            if (AddressOwner === address || address === sender) {
                coinsMeta[`${AddressOwner}${coinType}`] = {
                    amount:
                        (coinsMeta[`${AddressOwner}${coinType}`]?.amount || 0) +
                        amount,
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
