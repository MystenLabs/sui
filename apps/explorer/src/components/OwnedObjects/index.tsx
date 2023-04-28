// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import OwnedObject from './OwnedObject';

import { useGetOwnedObjects } from '~/hooks/useGetOwnedObjects';
import { Heading } from '~/ui/Heading';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Pagination, useCursorPagination } from '~/ui/Pagination';

export function OwnedObjects({ id }: { id: string }) {
    const ownedObjects = useGetOwnedObjects(id, {
        MatchNone: [{ StructType: '0x2::coin::Coin' }],
    });

    const { data, isError, isFetching, pagination } =
        useCursorPagination(ownedObjects);

    if (isError) {
        return (
            <div className="pt-2 font-sans font-semibold text-issue-dark">
                Failed to load NFTs
            </div>
        );
    }

    return (
        <div className="flex flex-col gap-4 pt-5">
            <Heading color="steel-darker" variant="heading4/semibold">
                NFTs
            </Heading>

            {isFetching ? (
                <LoadingSpinner />
            ) : (
                <div className="flex max-h-80 flex-col overflow-auto">
                    <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                        {data?.data.map((obj) => (
                            <OwnedObject obj={obj} key={obj?.data?.objectId} />
                        ))}
                    </div>
                </div>
            )}

            <Pagination {...pagination} />
        </div>
    );
}
