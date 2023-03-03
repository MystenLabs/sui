// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG, getTransactions } from '@mysten/sui.js';
import { useMemo } from 'react';

import { getEventsSummary, getAmount } from '_helpers';

import type { SuiTransactionResponse, SuiAddress } from '@mysten/sui.js';

export function useGetTransferAmount({
    txn,
    activeAddress,
}: {
    txn: SuiTransactionResponse;
    activeAddress: SuiAddress;
}) {
    const { effects, events } = txn;
    const { coins } = getEventsSummary(events, activeAddress);

    const suiTransfer = useMemo(() => {
        const txdetails = getTransactions(txn)[0];
        return getAmount(txdetails, effects, events)?.map(
            ({ amount, coinType, recipientAddress }) => {
                return {
                    amount: amount || 0,
                    coinType: coinType || SUI_TYPE_ARG,
                    receiverAddress: recipientAddress,
                };
            }
        );
    }, [txn, effects, events]);

    const transferAmount = useMemo(() => {
        return suiTransfer?.length
            ? suiTransfer
            : coins.filter(
                  ({ receiverAddress }) => receiverAddress === activeAddress
              );
    }, [suiTransfer, coins, activeAddress]);

    return suiTransfer ?? transferAmount;
}
