// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useAppsBackend } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

export function useSupportedCoins() {
	const { request } = useAppsBackend();

	return useQuery({
		queryKey: ['supported-coins-apps-backend'],
		queryFn: async () => request<{ supported: string[] }>('swap/coins'),
	});
}
