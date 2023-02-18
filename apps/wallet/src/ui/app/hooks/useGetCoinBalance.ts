// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '_hooks';
type GetCoinBalanceProps = {
    address?: SuiAddress | null;
    coinType: string;
};
export function useGetCoinBalance({ address, coinType }: GetCoinBalanceProps) {
    const rpc = useRpc();
    return useQuery(
        // combine address and coinType to make a unique key account for multiple addresses and coins
        ['coin-balance', address + coinType],
        async () => rpc.getBalance(address!, coinType),
        { enabled: !!address && !!coinType, refetchInterval: 4000 }
    );
}
