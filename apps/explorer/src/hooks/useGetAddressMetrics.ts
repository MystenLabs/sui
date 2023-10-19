// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { useQuery } from '@tanstack/react-query';

export function useGetAddressMetrics() {
	const client = useSuiClient();
	return useQuery({
		queryKey: ['home', 'addresses'],
		queryFn: () => client.getAddressMetrics(),
		gcTime: 24 * 60 * 60 * 1000,
		staleTime: Infinity,
		retry: 5,
	});
}
