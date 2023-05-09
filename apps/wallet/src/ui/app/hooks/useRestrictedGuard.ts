// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { useEffect } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';

export const RESTRICTED_ERROR = {
    status: 403,
    body: 'error code: 1020',
};

export function useRestrictedGuard() {
    const navigate = useNavigate();
    const location = useLocation();

    const { data } = useQuery(
        ['restricted-guard'],
        async () => {
            // NOTE: We use fetch directly here instead of the RPC layer because we don't want this instrumented,
            // and we also need to work with the response object directly.
            const res = await fetch('https://wallet-rpc.testnet.sui.io/', {
                method: 'POST',
                body: JSON.stringify({
                    id: 1,
                    method: 'sui_getLatestCheckpointSequenceNumber',
                    jsonrpc: '2.0',
                    params: [],
                }),
                headers: {
                    // Resetting accept makes the response non-HTML
                    accept: '',
                    'content-type': 'application/json',
                },
            });

            if (res.status === RESTRICTED_ERROR.status) {
                const body = await res.text();
                return { restricted: body === RESTRICTED_ERROR.body };
            }

            return { restricted: false };
        },
        {
            // Don't cache this query, we want the check every app start:
            cacheTime: 0,
            retry: 0,
        }
    );

    useEffect(() => {
        if (!data) return;
        if (data.restricted && location.pathname !== '/restricted') {
            navigate('/restricted', { replace: true });
        } else if (!data.restricted && location.pathname === '/restricted') {
            // If access is not restricted, but the user is on the restricted page, then we want to get them out of there:
            navigate('/', { replace: true });
        }
    }, [navigate, data, location.pathname]);

    return data?.restricted ?? false;
}
