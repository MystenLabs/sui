// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useAppsBackend } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

type BlocklistResponse = string[];

export function useDomainBlocklist() {
	const { request } = useAppsBackend();
	return useQuery({
		queryKey: ['apps-backend', 'domain-blocklist'],
		queryFn: () => request<BlocklistResponse>('blocklist'),
		refetchInterval: 24 * 5 * 1000, // refetch list every 5 minutes
	});
}
