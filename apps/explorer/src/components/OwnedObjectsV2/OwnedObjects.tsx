// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import OwnedObject from './components/OwnedObject';

import { useGetOwnedObjects } from '~/hooks/useGetOwnedObjects';
import { Heading } from '~/ui/Heading';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Pagination } from '~/ui/Pagination';

function OwnedObjects({ id }: { id: string }) {
    const [currentPage, setCurrentPage] = useState(0);
    const { data, isLoading, isError, hasNextPage, fetchNextPage, isFetching } =
        useGetOwnedObjects(id);

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
                    {isLoading || isFetching ? (
                        <LoadingSpinner />
                    ) : (
                        <div className="flex max-h-80 flex-col overflow-auto">
                            <div className="flex flex-wrap">
                                {data?.pages[currentPage]?.data.map((obj) => (
                                    <OwnedObject
                                        obj={obj}
                                        key={obj?.data?.objectId}
                                    />
                                ))}
                            </div>
                        </div>
                    )}

                    {(hasNextPage || data?.pages.length > 1) && (
                        <Pagination
                            onNext={() => {
                                if (isLoading || isFetching) {
                                    return;
                                }

                                // Make sure we are at the end before fetching another page
                                if (currentPage === data?.pages.length - 1) {
                                    fetchNextPage();
                                }
                                setCurrentPage(currentPage + 1);
                            }}
                            hasNext={Boolean(hasNextPage)}
                            hasPrev={currentPage !== 0}
                            onPrev={() => setCurrentPage(currentPage - 1)}
                            onFirst={() => setCurrentPage(1)}
                        />
                    )}
                </div>
            )}
        </div>
    );
}

export default OwnedObjects;
