// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useRpc } from './useRpc';

export function useGetTransaction(transactionId: string) {
    const rpc = useRpc();
    return useQuery(
        ['transactions-by-id', transactionId],
        async () => rpc.getTransactionWithEffects(transactionId),
        { enabled: !!transactionId }
    );
}
