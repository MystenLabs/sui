// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAppsBackend } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

import { useNetworkContext } from '~/context';
import { Network } from '~/utils/api/DefaultRpcClient';

export function useGetNetworkDegrade() {
	const { request } = useAppsBackend();
	const [network] = useNetworkContext();

	return useQuery({
		queryKey: ['apps-backend', 'monitor-network'],
		queryFn: () =>
			request<{ degraded: boolean }>('monitor-network', {
				project: 'EXPLORER',
			}),
		// Keep cached for 2 minutes:
		staleTime: 2 * 60 * 1000,
		retry: false,
		enabled: network === Network.MAINNET,
	});
}
