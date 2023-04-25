// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureValue } from '@growthbook/growthbook-react';
import { useRpcClient } from '@mysten/core';
import {
    type SuiTransactionBlockResponse,
    type SuiAddress,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { FEATURES } from '_src/shared/experimentation/features';

export function useQueryTransactionsByAddress(address: SuiAddress | null) {
    const rpc = useRpcClient();
    const refetchInterval = useFeatureValue(
        FEATURES.WALLET_ACTIVITY_REFETCH_INTERVAL,
        20_000
    );

    return useQuery(
        ['transactions-by-address', address],
        async () => {
            // combine from and to transactions
            const [txnIds, fromTxnIds] = await Promise.all([
                rpc.queryTransactionBlocks({
                    filter: {
                        ToAddress: address!,
                    },
                    options: {
                        showInput: true,
                        showEffects: true,
                        showEvents: true,
                    },
                }),
                rpc.queryTransactionBlocks({
                    filter: {
                        FromAddress: address!,
                    },
                    options: {
                        showInput: true,
                        showEffects: true,
                        showEvents: true,
                    },
                }),
            ]);

            const inserted = new Map();
            const uniqueList: SuiTransactionBlockResponse[] = [];

            [...txnIds.data, ...fromTxnIds.data]
                .sort(
                    (a, b) =>
                        Number(b.timestampMs ?? 0) - Number(a.timestampMs ?? 0)
                )
                .forEach((txb) => {
                    if (inserted.get(txb.digest)) return;
                    uniqueList.push(txb);
                    inserted.set(txb.digest, true);
                });

            return uniqueList;
        },
        { enabled: !!address, staleTime: 10 * 1000, refetchInterval }
    );
}
