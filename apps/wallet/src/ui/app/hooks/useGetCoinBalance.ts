// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '_hooks';

export function useGetCoinBalance(
    coinType: string,
    address?: SuiAddress | null
) {
    const rpc = useRpc();
    return useQuery(
        ['coin-balance', address, coinType],
        () => rpc.getBalance(address!, coinType),
        {
            enabled: !!address && !!coinType,
            refetchInterval: 4000,
        }
    );
}
