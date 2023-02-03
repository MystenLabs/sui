// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//show activity for a specific coin type

import { useMemo } from 'react';

import { TransactionCard } from '_components/transactions-card';
import { getEventsSummary } from '_helpers';

import type { SuiTransactionResponse, SuiAddress } from '@mysten/sui.js';

type CoinActivityCardProps = {
    activeCoinType: string;
    txn: SuiTransactionResponse;
    address: SuiAddress;
};

export function CoinActivityCard({
    activeCoinType,
    txn,
    address,
}: CoinActivityCardProps) {
    const { coins: eventsSummary } = getEventsSummary(txn.effects, address);
    const isCointype = useMemo(() => {
        return !!eventsSummary.find(
            ({ receiverAddress, coinType }) =>
                receiverAddress === address && coinType === activeCoinType
        );
    }, [activeCoinType, address, eventsSummary]);

    return isCointype ? <TransactionCard txn={txn} address={address} /> : null;
}
