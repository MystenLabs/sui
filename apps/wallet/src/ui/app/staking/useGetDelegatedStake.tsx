// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import type { DelegatedStake } from '@mysten/sui.js';

export function useGetDelegatedStake(address: string): UseQueryResult<DelegatedStake[], Error> {
	const rpc = useRpcClient();
	return useQuery({
		queryKey: ['validator', address],
		queryFn: () => rpc.getStakes({ owner: address }),
		staleTime: 10 * 1000,
		refetchInterval: 30 * 1000,
	});
}
