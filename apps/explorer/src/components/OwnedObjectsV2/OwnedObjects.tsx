// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiObjectResponse } from '@mysten/sui.js';
import { useEffect, useState } from 'react';

import OwnedObject from './components/OwnedObject';

import { useGetOwnedObjects } from '~/hooks/useGetOwnedObjects';
import { Heading } from '~/ui/Heading';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Pagination } from '~/ui/Pagination';

export const OBJECTS_PER_PAGE: number = 6;

function OwnerCoins({ id }: { id: string }): JSX.Element {
    const [results, setResults] = useState<SuiObjectResponse[]>([]);
    const [currentSlice, setCurrentSlice] = useState(1);
    const { data, isLoading, isError } = useGetOwnedObjects(id);

    useEffect(() => {
        if (data) {
            setResults(data?.data);
        }
    }, [data]);

    if (isError) {
        return (
            <div className="pt-2 font-sans font-semibold text-issue-dark">
                Failed to load NFTs
            </div>
        );
    }
    return (
        <div className="pl-7.5">
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
        </div>
    );
}

export default OwnerCoins;
