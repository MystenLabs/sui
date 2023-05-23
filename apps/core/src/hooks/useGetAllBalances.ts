// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

export function useGetAllBalances(
    address?: SuiAddress | null,
    refetchInterval?: number,
    staleTime?: number
) {
    const rpc = useRpcClient();
    return useQuery({
        queryKey: ['get-all-balance', address, staleTime, refetchInterval],
        queryFn: () => rpc.getAllBalances({ owner: address! }),
        enabled: !!address,
        refetchInterval,
        staleTime,
    });
}
