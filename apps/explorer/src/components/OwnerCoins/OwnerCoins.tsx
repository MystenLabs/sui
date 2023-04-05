// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useEffect, useState } from 'react';

import PaginationContainer from '../PaginationContainer/PaginationContainer';
import CoinView from './components/CoinView';

import type { CoinStruct } from '@mysten/sui.js';

import { Text } from '~/ui/Text';


export const COINS_PER_PAGE: number = 6;

function OwnerCoins({ id }: { id: string }) {
    const [results, setResults] = useState<CoinStruct[]>([]);
    const [isLoaded, setIsLoaded] = useState(false);
    const [isFail, setIsFail] = useState(false);
    const [currentSlice, setCurrentSlice] = useState(1);
    const rpc = useRpcClient();

    useEffect(() => {
        setIsFail(false);
        setIsLoaded(false);
        rpc.getAllCoins({ owner: id })
            .then((resp) => {
                setResults(resp.data);
                setIsLoaded(true);
            })
            .catch((err) => {
                setIsFail(true);
            });
    }, [id]);

    const uniqueCoinTypes = Array.from(
        new Set(results.map(({ coinType }) => coinType))
    );

    return (
        <PaginationContainer
            heading="Coins"
            isLoaded={isLoaded}
            isFail={isFail}
            itemsPerPage={COINS_PER_PAGE}
            paginatedContent={
                <div>
                    <div className="grid grid-cols-3 py-2 uppercase tracking-wider text-gray-80">
                        <Text variant="caption/medium">Type</Text>
                        <Text variant="caption/medium">Objects</Text>
                        <Text variant="caption/medium">Balance</Text>
                    </div>
                    <div>
                        {uniqueCoinTypes
                            .slice(
                                (currentSlice - 1) * COINS_PER_PAGE,
                                currentSlice * COINS_PER_PAGE
                            )
                            .map((coinType, index) => (
                                <CoinView
                                    coinType={coinType}
                                    objects={results.filter(
                                        (object) => object.coinType === coinType
                                    )}
                                    key={`${coinType}-${index}`}
                                />
                            ))}
                    </div>
                </div>
            }
            currentPage={currentSlice}
            setCurrentPage={(page: number) => setCurrentSlice(page)}
            totalItems={uniqueCoinTypes.length}
        />
    );
}

export default OwnerCoins;
