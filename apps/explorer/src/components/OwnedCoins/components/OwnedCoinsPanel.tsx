// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef } from 'react';

import CoinItem from './CoinItem';

import { useGetCoins } from '~/hooks/useGetCoins';
import { useOnScreen } from '~/hooks/useOnScreen';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

type CoinsPanelProps = {
    coinType: string;
    id: string;
};

function CoinsPanel({ coinType, id }: CoinsPanelProps): JSX.Element {
    const containerRef = useRef(null);
    const { isIntersecting } = useOnScreen(containerRef);
    const { data, isLoading, isFetching, fetchNextPage, hasNextPage } =
        useGetCoins(coinType, id);

    const isSpinnerVisible = hasNextPage || isLoading || isFetching;

    useEffect(() => {
        if (isIntersecting && hasNextPage && !isFetching) {
            fetchNextPage();
        }
    }, [isIntersecting, hasNextPage, isFetching, fetchNextPage]);

    return (
        <div>
            {data &&
                data.pages.map((page) =>
                    page.data.map((coin) => (
                        <CoinItem key={coin.coinObjectId} coin={coin} />
                    ))
                )}
            {isSpinnerVisible && (
                <div ref={containerRef}>
                    <LoadingSpinner />
                </div>
            )}
        </div>
    );
}
export default CoinsPanel;
