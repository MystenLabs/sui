// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useContext, useEffect, useState } from 'react';

import PaginationContainer from '../PaginationContainer/PaginationContainer';
import OwnedCoinView from './components/OwnedCoinView';

import type { CoinBalance } from '@mysten/sui.js';

import { Text } from '~/ui/Text';
import { AddressContext } from '~/pages/address-result/AddressResult';


export const COINS_PER_PAGE: number = 20;

function OwnedCoins({ id }: { id: string }) {
    const [uniqueCoins, setUniqueCoins] = useState<CoinBalance[]>([]);
    const [isLoaded, setIsLoaded] = useState(false);
    const [isFail, setIsFail] = useState(false);
    const [currentSlice, setCurrentSlice] = useState(1);
    const rpc = useRpcClient();

    useEffect(() => {
        setIsFail(false);
        setIsLoaded(false);
        rpc.getAllBalances({ owner: id, })
            .then((resp) => {
                setUniqueCoins(resp)
                setIsLoaded(true);
            })
            .catch((err) => {
                setIsFail(true);
            });
    }, [id, rpc]);

    return (
        <div className="max-h-[240px] overflow-scroll">
            <div className="grid grid-cols-3 py-2 uppercase tracking-wider text-gray-80">
                <Text variant="caption/medium">Type</Text>
                <Text variant="caption/medium">Objects</Text>
                <Text variant="caption/medium">Balance</Text>
            </div>
            <div>
                {uniqueCoins
                    .slice(
                        (currentSlice - 1) * COINS_PER_PAGE,
                        currentSlice * COINS_PER_PAGE
                    )
                    .map((coin, index) => (
                        <OwnedCoinView coin={coin} />
                    ))}
            </div>
        </div>
    );
}

export default OwnedCoins;
