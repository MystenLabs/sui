// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureValue } from '@growthbook/growthbook-react';
import { useRpcClient } from '@mysten/core';
import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { FEATURES } from '_src/shared/experimentation/features';

export function useGetCoinBalance(
    coinType: string,
    address?: SuiAddress | null
) {
    const rpc = useRpcClient();
    const refetchInterval = useFeatureValue(
        FEATURES.WALLET_BALANCE_REFETCH_INTERVAL,
        20_000
    );

    return useQuery({
        queryKey: ['coin-balance', address, coinType],
        queryFn: () => rpc.getBalance({ owner: address!, coinType }),
        enabled: !!address && !!coinType,
        refetchInterval,
        staleTime: 5_000,
    });
}
