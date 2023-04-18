// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { QueryClient } from '@tanstack/react-query';
import {
    type PersistedClient,
    type Persister,
} from '@tanstack/react-query-persist-client';
import { get, set, del } from 'idb-keyval';

/**
 * Creates an Indexed DB persister
 * @see https://developer.mozilla.org/en-US/docs/Web/API/IndexedDB_API
 */
function createIDBPersister(idbValidKey: IDBValidKey = 'reactQuery') {
    return {
        persistClient: async (client: PersistedClient) => {
            set(idbValidKey, client);
        },
        restoreClient: async () => await get<PersistedClient>(idbValidKey),
        removeClient: async () => {
            await del(idbValidKey);
        },
    } as Persister;
}

export const queryClient = new QueryClient({
    defaultOptions: {
        queries: {
            cacheTime: 5 * 60 * 1000,
            // We default the stale time to 1 minutes, which is an arbitrary number selected to
            // strike the balance between stale data and cache hits.
            // Individual queries can override this value based on their caching needs.
            staleTime: 60 * 1000,
            refetchInterval: false,
            refetchIntervalInBackground: false,
            // TODO: re-enable/remove when api is healthy ===>
            retry: false,
            refetchOnWindowFocus: false,
            //<======
            refetchOnMount: true,
        },
    },
});

export const persister = createIDBPersister();
