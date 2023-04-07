// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type PaginatedCoins, type CoinStruct } from '@mysten/sui.js';
import { useState, useEffect } from 'react';

import CoinItem from './CoinItem';

import { useGetCoins } from '~/hooks/useGetCoins';
import { useIntersectionObserver } from '~/hooks/useIntersectionObserver';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

type CoinsPanelProps = {
    coinType: string;
    id: string;
};

function CoinsPanel({ coinType, id }: CoinsPanelProps): JSX.Element {
    const [coinObjects, setCoinObjects] = useState<CoinStruct[]>([]);
    const [hasNextPage, setHasNextPage] = useState<boolean>(false);
    const [nextCursor, setNextCursor] = useState<string | null>();
    const { data, refetch, isLoading, isFetching } = useGetCoins(
        coinType,
        id,
        nextCursor
    );

    const update = (resp: PaginatedCoins) => {
        setCoinObjects((coinObjects) => [...coinObjects, ...resp.data]);
        setHasNextPage(resp.hasNextPage);
        setNextCursor(resp.nextCursor);
    };

    useEffect(() => {
        if (data) {
            update(data);
        }
    }, [data]);

    const { containerRef, isIntersecting, observer } =
        useIntersectionObserver();

    useEffect(() => {
        if (
            isIntersecting &&
            hasNextPage &&
            nextCursor &&
            !isLoading &&
            !isFetching
        ) {
            refetch();
            observer?.disconnect();
        }
    }, [
        isIntersecting,
        hasNextPage,
        nextCursor,
        isLoading,
        isFetching,
        refetch,
        observer,
    ]);

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
