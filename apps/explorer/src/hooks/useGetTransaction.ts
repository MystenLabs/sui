// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

export function useGetTransaction(transactionId: string) {
    const rpc = useRpcClient();
    return useQuery(
        ['transactions-by-id', transactionId],
        async () =>
            rpc.getTransactionResponse(transactionId, {
                showInput: true,
                showEffects: true,
                showEvents: true,
            }),
        { enabled: !!transactionId }
    );
}
