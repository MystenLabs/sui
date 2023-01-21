// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';

import { useNetwork } from '~/context';
import { Network } from '~/utils/api/DefaultRpcClient';

export function useAppsBackend() {
    const [network] = useNetwork();

    return useCallback(
        async <T>(
            path: string,
            queryString: Record<string, any>
        ): Promise<T> => {
            const query = new URLSearchParams(queryString);
            const res = await fetch(
                network === Network.LOCAL
                    ? `http://localhost:3003/${path}?${query}`
                    : `https://apps-backend.sui.io/${path}?${query}`
            );

            if (!res.ok) {
                throw new Error('Unexpected response');
            }

            return res.json();
        },
        [network]
    );
}
