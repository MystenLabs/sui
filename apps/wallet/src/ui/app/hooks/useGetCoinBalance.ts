// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type CoinBalance, type SuiAddress } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from '_hooks';
type GetCoinBalanceProps = {
    address: SuiAddress;
    coinType: string;
};
export function useGetCoinBalance({
    address,
    coinType,
}: GetCoinBalanceProps): UseQueryResult<CoinBalance, unknown> {
    const rpc = useRpc();
    const response = useQuery(
        // combine address and coinType to make a unique key account for multiple addresses and coins
        ['coin-balance', address + coinType],
        async () => rpc.getBalance(address, coinType),
        { enabled: !!address && !!coinType, refetchInterval: 20000 }
    );

    return response;
}
