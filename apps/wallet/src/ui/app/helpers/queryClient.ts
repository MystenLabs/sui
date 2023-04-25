// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { QueryClient } from '@tanstack/react-query';
import {
    type PersistedClient,
    type Persister,
} from '@tanstack/react-query-persist-client';
import { get, set, del } from 'idb-keyval';

export const queryClient = new QueryClient({
    defaultOptions: {
        queries: {
            // Only retry once by default:
            retry: 1,
            // Default stale time to 30 seconds, which seems like a sensible tradeoff between network requests and stale data.
            staleTime: 30 * 1000,
            // Default cache time to 24 hours, so that data will remain in the cache and improve wallet loading UX.
            cacheTime: 24 * 60 * 60 * 1000,
            // Disable automatic interval fetching
            refetchInterval: 0,
            refetchIntervalInBackground: false,
            refetchOnWindowFocus: false,

            refetchOnMount: true,
        },
    },
});

function createIDBPersister(idbValidKey: IDBValidKey) {
    return {
        persistClient: async (client: PersistedClient) => {
            set(idbValidKey, client);
        },
        restoreClient: async () => {
            return await get<PersistedClient>(idbValidKey);
        },
        removeClient: async () => {
            await del(idbValidKey);
        },
    } as Persister;
}

export const persister = createIDBPersister('queryClient.v1');
