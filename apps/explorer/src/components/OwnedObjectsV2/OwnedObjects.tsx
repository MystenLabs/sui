// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiObjectResponse, Coin } from '@mysten/sui.js';
import { useEffect, useState } from 'react';

import OwnedObject from './components/OwnedObject';

import { Heading } from '~/ui/Heading';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Pagination } from '~/ui/Pagination';

export const OBJECTS_PER_PAGE: number = 6;

function OwnerCoins({ id }: { id: string }) {
    const [results, setResults] = useState<SuiObjectResponse[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [isFail, setIsFail] = useState(false);
    const [currentSlice, setCurrentSlice] = useState(1);
    const rpc = useRpcClient();

    useEffect(() => {
        setIsLoading(true);
        rpc.getOwnedObjects({
            owner: id,
            options: {
                showType: true,
                showContent: true,
                showDisplay: true,
            },
        })
            .then((objects) => {
                const res = objects.data.filter((resp) => !Coin.isCoin(resp));
                setResults(res);
                setIsLoading(false);
            })
            .catch((err) => {
                setIsFail(true);
            });
    }, [id, rpc]);

    if (isFail) {
        return (
            <div className="pt-2 font-sans font-semibold text-issue-dark">
                Failed to load NFTs
            </div>
        );
    }
    return (
        <>
            {isLoading ? (
                <LoadingSpinner />
            ) : (
                <div className="flex flex-col space-y-5 pt-5 text-left xl:pr-10">
                    <Heading color="gray-90" variant="heading4/semibold">
                        NFTs
                    </Heading>

                    <div className="flex max-h-80 flex-col overflow-auto">
                        <div className="flex flex-wrap">
                            {results
                                .slice(
                                    (currentSlice - 1) * OBJECTS_PER_PAGE,
                                    currentSlice * OBJECTS_PER_PAGE
                                )
                                .map((obj) => (
                                    <OwnedObject
                                        obj={obj}
                                        key={obj?.data?.objectId}
                                    />
                                ))}
                        </div>
                    </div>
                    {results.length > OBJECTS_PER_PAGE && (
                        <Pagination
                            onNext={() => setCurrentSlice(currentSlice + 1)}
                            hasNext={
                                currentSlice !==
                                Math.ceil(results.length / OBJECTS_PER_PAGE)
                            }
                            hasPrev={currentSlice !== 1}
                            onPrev={() => setCurrentSlice(currentSlice - 1)}
                            onFirst={() => setCurrentSlice(1)}
                        />
                    )}
                </div>
            )}
        </>
    );
}

export default OwnerCoins;
