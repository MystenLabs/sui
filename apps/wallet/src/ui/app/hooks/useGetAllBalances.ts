// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

export function useGetAllBalances(address?: SuiAddress | null) {
    const rpc = useRpcClient();
    return useQuery(
        ['get-all-balance', address],
        () => rpc.getAllBalances({ owner: address! }),
        // refetchInterval is set to 4 seconds
        { enabled: !!address, refetchInterval: 4000 }
    );
}
