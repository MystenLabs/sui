// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Sentry from '@sentry/react';
import { createContext, useEffect } from 'react';
// eslint-disable-next-line no-restricted-imports
import { useSearchParams } from 'react-router-dom';

import { Network } from './utils/api/DefaultRpcClient';
import { growthbook } from './utils/growthbook';
import { queryClient } from './utils/queryClient';

export const DEFAULT_NETWORK =
    import.meta.env.VITE_NETWORK ||
    (import.meta.env.DEV ? Network.LOCAL : Network.TESTNET);

export const NetworkContext = createContext<
    [Network | string, (network: Network | string) => void]
>(['', () => null]);

export function useNetwork(): [string, (network: Network | string) => void] {
    const [searchParams, setSearchParams] = useSearchParams();
    const network = getNetworkName(searchParams);

    const setNetwork = (network: Network | string) => {
        setSearchParams({ network: network.toLowerCase() });
    };

    useEffect(() => {
        console.log('clearing cache', network);
        // When the network changes (either from users changing the network manually or
        // navigating back and forth between pages), we need to clear out our query cache
        queryClient.cancelQueries();
        queryClient.clear();
        return () => console.log('dismount');
    }, [network]);

    useEffect(() => {
        growthbook.setAttributes({
            network,
            environment: import.meta.env.VITE_VERCEL_ENV,
        });

        Sentry.setContext('network', {
            network,
        });
    }, [network]);

    return [network, setNetwork];
}

function getNetworkName(searchParams: URLSearchParams) {
    const networkParam = searchParams.get('network');
    const upperCasedNetwork = networkParam?.toUpperCase();

    if (
        upperCasedNetwork &&
        (Object.values(Network) as string[]).includes(upperCasedNetwork)
    ) {
        return upperCasedNetwork;
    }
    return networkParam ?? DEFAULT_NETWORK;
}
