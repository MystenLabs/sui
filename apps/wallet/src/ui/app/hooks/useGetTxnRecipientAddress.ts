// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import { getEventsSummary, getAmount } from '_helpers';

import type { SuiTransactionResponse, SuiAddress } from '@mysten/sui.js';

type Props = {
    txn: SuiTransactionResponse;
    address: SuiAddress;
};

export function useGetTxnRecipientAddress({ txn, address }: Props) {
    const { certificate, effects } = txn;

    const eventsSummary = useMemo(() => {
        const { coins } = getEventsSummary(effects, address);
        return coins;
    }, [effects, address]);

    const amountByRecipient = getAmount(
        certificate.data.transactions[0],
        txn.effects
    );

    const recipientAddress = useMemo(() => {
        const transferObjectRecipientAddress =
            amountByRecipient &&
            amountByRecipient?.find(
                ({ recipientAddress }) => recipientAddress !== address
            )?.recipientAddress;
        const receiverAddr =
            eventsSummary &&
            eventsSummary.find(
                ({ receiverAddress }) => receiverAddress !== address
            )?.receiverAddress;

        return (
            receiverAddr ??
            transferObjectRecipientAddress ??
            certificate.data.sender
        );
    }, [address, amountByRecipient, certificate.data.sender, eventsSummary]);

    return recipientAddress;
}
