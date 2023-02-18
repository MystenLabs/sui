// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type CoinBalance, type SuiAddress } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from '_hooks';
type GetCoinBalanceProps = {
    address: SuiAddress | null;
};
export function useGetAllBalance({
    address,
}: GetCoinBalanceProps): UseQueryResult<CoinBalance[], unknown> {
    const rpc = useRpc();
    const response = useQuery(
        ['get-all-balance', address],
        () => rpc.getAllBalances(address!),
        // refetchInterval is set to 2 seconds
        { enabled: !!address, refetchInterval: 20000 }
    );

    return response;
}
