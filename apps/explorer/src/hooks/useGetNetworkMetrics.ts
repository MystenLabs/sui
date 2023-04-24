// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

export function useGetNetworkMetrics() {
    const rpc = useRpcClient();
    return useQuery(['home', 'metrics'], () => rpc.getNetworkMetrics(), {
        cacheTime: 24 * 60 * 60 * 1000,
        staleTime: Infinity,
        retry: 5,
    });
}
