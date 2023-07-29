// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Sentry from '@sentry/react';
import { createContext, useContext, useLayoutEffect, useMemo } from 'react';
// eslint-disable-next-line no-restricted-imports
import { useSearchParams } from 'react-router-dom';

import { Network } from './utils/api/DefaultRpcClient';
import { growthbook } from './utils/growthbook';
import { queryClient } from './utils/queryClient';

export const DEFAULT_NETWORK =
	import.meta.env.VITE_NETWORK || (import.meta.env.DEV ? Network.LOCAL : Network.MAINNET);

export const NetworkContext = createContext<
	[Network | string, (network: Network | string) => void]
>(['', () => null]);

export function useNetworkContext() {
	return useContext(NetworkContext);
}

export function useNetwork(): [string, (network: Network | string) => void] {
	const [searchParams, setSearchParams] = useSearchParams();

	const network = useMemo(() => {
		const networkParam = searchParams.get('network');

		if (networkParam && (Object.values(Network) as string[]).includes(networkParam.toUpperCase())) {
			return networkParam.toUpperCase();
		}

		return networkParam ?? DEFAULT_NETWORK;
	}, [searchParams]);

	const setNetwork = (network: Network | string) => {
		// When resetting the network, we reset the query client at the same time:
		queryClient.cancelQueries();
		queryClient.clear();

		setSearchParams({ network: network.toLowerCase() });
	};

	useLayoutEffect(() => {
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
