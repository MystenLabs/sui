// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiTransactionResponse, type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '_hooks';

const dedupe = (arr: string[]) => Array.from(new Set(arr));

export function useGetTransactionsByAddress(address: SuiAddress | null) {
    const rpc = useRpc();

    const response = useQuery<SuiTransactionResponse[], Error>(
        ['transactions-by-address', address],
        async () => {
            if (!address) return [];
            // combine the two responses into one
            const [txnIdDs, fromTxnIds] = await Promise.all([
                rpc.getTransactions({
                    ToAddress: address,
                }),
                rpc.getTransactions({
                    FromAddress: address,
                }),
            ]);
            const resp = await rpc.getTransactionWithEffectsBatch(
                dedupe([...txnIdDs.data, ...fromTxnIds.data])
            );

            return resp.sort(
                (a, b) => (b?.timestamp_ms || 0) - (a.timestamp_ms || 0)
            );
        },
        { enabled: !!address, staleTime: 10 * 1000 }
    );
    return response;
}
