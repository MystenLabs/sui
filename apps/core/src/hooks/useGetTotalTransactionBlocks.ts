// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { useQuery } from '@tanstack/react-query';

const defaultOptions = {
    cacheTime: 24 * 60 * 60 * 1000,
    staleTime: Infinity,
    retry: 5,
};
export function useGetTotalTransactionBlocks(options = defaultOptions) {
    const rpc = useRpcClient();
    return useQuery(
        ['home', 'transaction-count'],
        () => rpc.getTotalTransactionBlocks(),
        { ...options }
    );
}
