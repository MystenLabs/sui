// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type PaginatedCoins, type CoinStruct } from '@mysten/sui.js';
import { useState, useRef, useEffect, useMemo } from 'react';

import CoinItem from './CoinItem';

import { LoadingSpinner } from '~/ui/LoadingSpinner';

type CoinsPanelProps = {
    coinType: string;
    id: string;
};

const MAX_COIN_LIMIT = 10;

function CoinsPanel({ coinType, id }: CoinsPanelProps): JSX.Element {
    const [coinObjects, setCoinObjects] = useState<CoinStruct[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [hasNextPage, setHasNextPage] = useState<boolean>(false);
    const [nextCursor, setNextCursor] = useState<string | null>();
    const [isVisible, setIsVisible] = useState(false);
    const rpc = useRpcClient();

    const update = (resp: PaginatedCoins) => {
        setCoinObjects((coinObjects) => [...coinObjects, ...resp.data]);
        setHasNextPage(resp.hasNextPage);
        setNextCursor(resp.nextCursor);
        setIsLoading(false);
    };

    useEffect(() => {
        setIsLoading(true);
        rpc.getCoins({ owner: id, coinType, limit: MAX_COIN_LIMIT }).then((resp) => {
            update(resp);
        });
    }, [id, coinType, rpc]);

    const containerRef = useRef(null);

    const options = useMemo(
        () => ({
            root: null,
            rootMargin: '0px',
            threshold: 1,
        }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        undefined
    );

    const observer = useMemo(
        () =>
            new IntersectionObserver((entries) => {
                const last = entries.pop();
                last && setIsVisible(last.isIntersecting);
            }, options),
        [options]
    );

    useEffect(() => {
        const current = containerRef.current;

        if (containerRef.current) {
            observer.observe(containerRef.current);
        }

        return () => {
            if (current) {
                observer.unobserve(current);
            }
        };
    }, [containerRef, observer]);

    useEffect(() => {
        if (isVisible && hasNextPage && nextCursor && !isLoading) {
            setIsLoading(true);
            rpc.getCoins({
                owner: id,
                coinType,
                limit: MAX_COIN_LIMIT,
                cursor: nextCursor,
            }).then((resp) => {
                update(resp);
            });
        }
    }, [isVisible, hasNextPage, nextCursor, isLoading, coinType, id, rpc]);

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
            {isLoading && <LoadingSpinner />}
        </div>
    );
}
export default CoinsPanel;
