// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getTransactionSender,
    STAKING_REQUEST_EVENT,
    SuiTransactionBlockResponse,
    TransactionEvents,
    UNSTAKING_REQUEST_EVENT,
} from '@mysten/sui.js';

function getEventType(events: TransactionEvents = []) {
    return events.find(({ type }) =>
        [STAKING_REQUEST_EVENT, UNSTAKING_REQUEST_EVENT].includes(type)
    )?.type;
}

export function useGetTransferLabel(
    txn: SuiTransactionBlockResponse,
    currentAddress: string
) {
    const senderAddress = getTransactionSender(txn);
    const type = getEventType(txn.events);

    switch (type) {
        case STAKING_REQUEST_EVENT:
            return 'Staked';
        case UNSTAKING_REQUEST_EVENT:
            return 'Unstaked';
        default:
            return senderAddress === currentAddress ? 'Sent' : 'Received';
    }
}
