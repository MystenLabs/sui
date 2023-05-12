// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress, SuiObjectResponse } from '@mysten/sui.js';
import { useRpcClient } from '../api/RpcClientContext';
import { useGetOriginByteKioskContents } from './useGetOriginByteKioskContents';
import { useInfiniteQuery } from '@tanstack/react-query';
import { useEffect, useRef } from 'react';

interface UseGetOriginByteKioskContentsParams {
    address?: SuiAddress | null;
    maxObjectRequests?: number;
}

const MAX_OBJECTS_PER_REQ = 6;

// todo: this is a workaround to get kiosk contents to display in Explorer
// with our current strategy for pagination. we should remove this when we have proper
// APIs for kiosks
export function useGetOwnedObjectsWithKiosks({
    address,
    maxObjectRequests = MAX_OBJECTS_PER_REQ,
}: UseGetOriginByteKioskContentsParams) {
    const rpc = useRpcClient();
    const { data: kioskContents, isFetched } =
        useGetOriginByteKioskContents(address);

    const kiosk = useRef<SuiObjectResponse[]>();
    useEffect(() => {
        if (kioskContents) {
            kiosk.current = kioskContents;
        }
    }, [kioskContents]);

    return useInfiniteQuery(
        ['get-owned-objects-with-kiosks', address, maxObjectRequests],
        async ({ pageParam }) => {
            const ownedObjects = await rpc.getOwnedObjects({
                owner: address!,
                filter: { MatchNone: [{ StructType: '0x2::coin::Coin' }] },
                options: {
                    showType: true,
                    showContent: true,
                    showDisplay: true,
                },
                limit: maxObjectRequests,
                cursor: kioskContents?.length ? undefined : pageParam,
            });

            // if there are no kiosk contents just return normally
            if (!kiosk.current?.length) return ownedObjects;

            // set data to the kiosk contents and mutate the array to remove the items
            const data: SuiObjectResponse[] =
                kiosk.current?.splice(0, maxObjectRequests) ?? [];

            // if we're out of kiosk items to display, return owned objects
            if (data.length < maxObjectRequests) {
                const diff = maxObjectRequests - data.length;
                data.push(...ownedObjects.data.splice(0, diff));
            }

            return {
                ...ownedObjects,
                data,
            };
        },
        {
            getNextPageParam: (lastPage) =>
                lastPage.hasNextPage ? lastPage.nextCursor : undefined,
            enabled: isFetched,
        }
    );
}
