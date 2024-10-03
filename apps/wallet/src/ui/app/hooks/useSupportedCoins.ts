// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { USDC_TYPE_ARG, W_USDC_TYPE_ARG } from '_pages/swap/utils';
import { useAppsBackend } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui/utils';
import { useQuery } from '@tanstack/react-query';

const DEFAULT_SUPPORTED_COINS = [SUI_TYPE_ARG, USDC_TYPE_ARG, W_USDC_TYPE_ARG];

export function useSupportedCoins() {
	const { request } = useAppsBackend();

	return useQuery({
		queryKey: ['supported-coins'],
		queryFn: async () => request<{ supported: string[] }>('swap/coins'),
		initialData: { supported: DEFAULT_SUPPORTED_COINS },
	});
}
