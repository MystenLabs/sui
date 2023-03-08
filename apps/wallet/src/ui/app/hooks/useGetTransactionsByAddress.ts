// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

const dedupe = (arr: string[]) => Array.from(new Set(arr));

export function useGetTransactionsByAddress(address: SuiAddress | null) {
    const rpc = useRpcClient();

    return useQuery(
        ['transactions-by-address', address],
        async () => {
            // combine from and to transactions
            const [txnIds, fromTxnIds] = await Promise.all([
                rpc.getTransactions({
                    ToAddress: address!,
                }),
                rpc.getTransactions({
                    FromAddress: address!,
                }),
            ]);
            const resp = await rpc.getTransactionResponseBatch(
                dedupe([...txnIds.data, ...fromTxnIds.data]),
                {
                    showInput: true,
                    showTimestamp: true,
                    showEffects: true,
                    showEvents: true,
                }
            );

            return resp.sort(
                // timestamp could be null, so we need to handle
                (a, b) => (b.timestampMs || 0) - (b.timestampMs || 0)
            );
        },
        { enabled: !!address, staleTime: 10 * 1000 }
    );
}
