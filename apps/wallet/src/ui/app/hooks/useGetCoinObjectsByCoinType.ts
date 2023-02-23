// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';

import { useRpc, useGetCoins, useAppSelector } from '_hooks';

//This combines the useGetCoins and useGetObjectBatch hooks to get the coins and then the objects for those coins
// specifically for the Coin transfer page were we need to get the objects for each coin type
export function useGetCoinObjectsByCoinType(coinType: string) {
    const activeAddress = useAppSelector(({ account: { address } }) => address);
    const rpc = useRpc();

    const {
        data: coins,
        error: getCoinsError,
        isLoading: getCoinsLoading,
    } = useGetCoins(coinType, activeAddress!);

    const coinsObjectIds = useMemo(
        () => coins?.map(({ coinObjectId }) => coinObjectId) || [],
        [coins]
    );

    const { data, isLoading, error } = useQuery(
        ['get-object-batch', coinsObjectIds],
        () => rpc.getObjectBatch(coinsObjectIds),
        { enabled: !!coinsObjectIds.length }
    );

    return {
        data,
        isLoading: isLoading || getCoinsLoading,
        error: error || getCoinsError,
    };
}
