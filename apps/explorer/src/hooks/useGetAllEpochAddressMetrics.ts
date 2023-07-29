// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiClient } from '@mysten/sui.js/client';
import { useQuery } from '@tanstack/react-query';

export function useGetAllEpochAddressMetrics(
	...input: Parameters<SuiClient['getAllEpochAddressMetrics']>
) {
	const rpc = useRpcClient();
	return useQuery({
		queryKey: ['get', 'all', 'epoch', 'addresses', ...input],
		queryFn: () => rpc.getAllEpochAddressMetrics(...input),
	});
}
