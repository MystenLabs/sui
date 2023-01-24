// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress, type SuiTransactionResponse } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from '_hooks';

const REFETCH_INTERVAL = 2000;

export function useGetTransactionsByAddress(
    address?: SuiAddress | null
): UseQueryResult<SuiTransactionResponse[], Error> {
    const rpc = useRpc();

    return useQuery(
        ['transactions-by-address', address],
        async () => {
            const currentAddress = address ?? '';

            const txnsIds = await rpc.getTransactions({
                ToAddress: currentAddress,
            });
            return rpc.getTransactionWithEffectsBatch(txnsIds.data);
        },
        {
            enabled: address != null,
            refetchOnMount: true,
            refetchInterval: REFETCH_INTERVAL,
        }
    );
}
