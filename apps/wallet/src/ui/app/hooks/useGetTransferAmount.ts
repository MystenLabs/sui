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
    const { effects } = txn;
    const { coins } = getEventsSummary(effects, activeAddress);

    const suiTransfer = useMemo(() => {
        const txdetails = getTransactions(txn)[0];
        return getAmount(txdetails, effects)?.map(
            ({ amount, coinType, recipientAddress }) => {
                return {
                    amount: amount || 0,
                    coinType: coinType || SUI_TYPE_ARG,
                    receiverAddress: recipientAddress,
                };
            }
        );
    }, [txn, effects]);

    const transferAmount = useMemo(() => {
        return suiTransfer?.length
            ? suiTransfer
            : coins.filter(
                  ({ receiverAddress }) => receiverAddress === activeAddress
              );
    }, [suiTransfer, coins, activeAddress]);

    return suiTransfer ?? transferAmount;
}
