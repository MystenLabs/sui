// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '_hooks';

export function useGetAllBalances(address?: SuiAddress | null) {
    const rpc = useRpc();
    return useQuery(
        ['get-all-balance', address],
        () => rpc.getAllBalances(address!),
        // refetchInterval is set to 4 seconds
        { enabled: !!address, refetchInterval: 4000 }
    );
}
