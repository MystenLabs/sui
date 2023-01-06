// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from '_hooks';

import type { GetTxnDigestsResponse } from '@mysten/sui.js';

// stale after 2 seconds
const TRANSACTION_STALE_TIME = 2 * 1000;

export function useGetTransactions(
    address: string
): UseQueryResult<GetTxnDigestsResponse, unknown> {
    const rpc = useRpc();
    const response = useQuery(
        ['txnActivities', address],
        async () => {
            return rpc.getTransactionsForAddress(address, true);
        },
        { enabled: !!address, staleTime: TRANSACTION_STALE_TIME }
    );

    

    return response;
}
