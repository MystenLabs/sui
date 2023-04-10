// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type CoinStruct } from '@mysten/sui.js';
import { useState, useEffect, useRef } from 'react';

import CoinItem from './CoinItem';

import { useGetCoins } from '~/hooks/useGetCoins';
import { useOnScreen } from '~/hooks/useOnScreen';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

type CoinsPanelProps = {
    coinType: string;
    id: string;
};

function CoinsPanel({ coinType, id }: CoinsPanelProps): JSX.Element {
    const [coinObjects, setCoinObjects] = useState<CoinStruct[]>([]);
    const containerRef = useRef(null);
    const { isIntersecting } = useOnScreen(containerRef);
    const { data, isLoading, isFetching, fetchNextPage, hasNextPage } =
        useGetCoins(coinType, id);

    useEffect(() => {
        if (data) {
            let coins: CoinStruct[] = [];
            data.pages.forEach((page) => {
                coins = [...coins, ...page.data];
            });
            setCoinObjects(coins);
        }
    }, [data]);

    useEffect(() => {
        if (isIntersecting && hasNextPage && !isFetching) {
            fetchNextPage();
        }
    }, [isIntersecting, hasNextPage, isFetching, fetchNextPage]);

    return (
        <div>
            {coinObjects.map((obj, index) => {
                if (index === coinObjects.length - 1) {
                    return (
                        <div key={obj.coinObjectId} ref={containerRef}>
                            <CoinItem coin={obj} />
                        </div>
                    );
                }
                return <CoinItem key={obj.coinObjectId} coin={obj} />;
            })}
            {(isLoading || isFetching) && <LoadingSpinner />}
        </div>
    );
}
export default CoinsPanel;
