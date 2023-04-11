// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

const dedupe = (arr: string[]) => Array.from(new Set(arr));

export function useQueryTransactionsByAddress(address: SuiAddress | null) {
    const rpc = useRpcClient();

    return useQuery(
        ['transactions-by-address', address],
        async () => {
            // combine from and to transactions
            const [txnIds, fromTxnIds] = await Promise.all([
                rpc.queryTransactionBlocks({
                    filter: {
                        ToAddress: address!,
                    },
                }),
                rpc.queryTransactionBlocks({
                    filter: {
                        FromAddress: address!,
                    },
                }),
            ]);
            // TODO: replace this with queryTransactions
            // It seems to be expensive to fetch all transaction data at once though
            const resp = await rpc.multiGetTransactionBlocks({
                digests: dedupe(
                    [...txnIds.data, ...fromTxnIds.data].map((x) => x.digest)
                ),
                options: {
                    showInput: true,
                    showEffects: true,
                    showEvents: true,
                },
            });

            return resp.sort(
                // timestamp could be null, so we need to handle
                (a, b) => +(b.timestampMs || 0) - +(a.timestampMs || 0)
            );
        },
        { enabled: !!address, staleTime: 10 * 1000 }
    );
}
