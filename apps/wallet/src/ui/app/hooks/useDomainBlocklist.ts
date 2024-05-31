// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useAppsBackend } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

export function useCheckBlocklist(hostname?: string) {
	const { request } = useAppsBackend();
	return useQuery({
		queryKey: ['apps-backend', 'blocklist', hostname],
		queryFn: () => request<{ block: boolean }>(`blocklist/${hostname}`),
		enabled: !!hostname,
	});
}
