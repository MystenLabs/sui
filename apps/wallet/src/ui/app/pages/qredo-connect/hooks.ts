// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { useQredoAPI } from '../../hooks/useQredoAPI';
import { QREDO_PENDING_REQUEST_KEY_COMMON } from './utils';
import { type GetWalletsParams } from '_src/shared/qredo-api';

export function useQredoUIPendingRequest(requestID?: string) {
    const backgroundClient = useBackgroundClient();
    return useQuery({
        queryKey: [...QREDO_PENDING_REQUEST_KEY_COMMON, requestID],
        queryFn: async () =>
            await backgroundClient.fetchPendingQredoConnectRequest(requestID!),
        // events from background service will invalidate this key (when qredo pending requests change)
        staleTime: Infinity,
        enabled: !!requestID,
        meta: { skipPersistedCache: true },
    });
}

export function useFetchQredoAccounts(
    qredoID?: string,
    enabled?: boolean,
    params?: GetWalletsParams
) {
    const [api, isAPILoading, apiInitError] = useQredoAPI(qredoID);
    return useQuery({
        queryKey: ['qredo', 'fetch', 'accounts', qredoID, api, apiInitError],
        queryFn: async () => {
            if (api) {
                return (await api.getWallets(params)).wallets;
            }
            throw apiInitError
                ? apiInitError
                : new Error('Qredo API initialization failed');
        },
        enabled:
            !!qredoID &&
            (enabled ?? true) &&
            !isAPILoading &&
            !!(api || apiInitError),
    });
}
