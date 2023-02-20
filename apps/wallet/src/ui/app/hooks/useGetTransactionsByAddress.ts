// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiTransactionResponse, type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '_hooks';

export function useGetTransactionsByAddress(address: SuiAddress | null) {
    const rpc = useRpc();

    const response = useQuery<SuiTransactionResponse[], Error>(
        ['transactions-by-address', address],
        async () => {
            if (!address) return [];
            const txnIdDs = await rpc.getTransactions({
                ToAddress: address,
            });
            return rpc.getTransactionWithEffectsBatch(txnIdDs.data);
        },
        { enabled: !!address, staleTime: 10 * 1000 }
    );
    return response;
}
