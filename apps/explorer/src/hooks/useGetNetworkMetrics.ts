// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { useQuery } from '@tanstack/react-query';

export function useGetNetworkMetrics() {
	const client = useSuiClient();
	return useQuery({
		queryKey: ['home', 'metrics'],
		queryFn: () => client.getNetworkMetrics(),
		cacheTime: 24 * 60 * 60 * 1000,
		staleTime: Infinity,
		retry: 5,
	});
}
