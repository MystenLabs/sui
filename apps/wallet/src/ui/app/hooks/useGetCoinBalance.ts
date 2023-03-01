// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { toast } from 'react-hot-toast';

export function useGetCoinBalance(
    coinType: string,
    address?: SuiAddress | null
) {
    const rpc = useRpcClient();
    return useQuery(
        ['coin-balance', address, coinType],
        () => rpc.getBalance(address!, coinType),
        {
            enabled: !!address && !!coinType,
            refetchInterval: 4000,
            onError: (error) => {
                toast.error((error as Error).message);
            },
        }
    );
}
