// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import type { DelegatedStake } from '@mysten/sui.js/client';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

type UseGetDelegatedStakeOptions = {
	autoRefetch?: boolean;
};

const STALE_TIME = 10_000;
const REFETCH_INTERVAL = 30_000;

export function useGetDelegatedStake(
	address: string,
	options?: UseGetDelegatedStakeOptions,
): UseQueryResult<DelegatedStake[], Error> {
	const client = useSuiClient();
	const { autoRefetch = false } = options || {};

	// Generalized query options
	const defaultQueryOptions = {
		staleTime: STALE_TIME,
		refetchInterval: REFETCH_INTERVAL,
	};

	const refetchQueryOptions = {
		staleTime: Infinity,
		refetchInterval: false,
	} as const;

	return useQuery({
		queryKey: ['delegated-stakes', address],
		queryFn: () => client.getStakes({ owner: address }),
		...(autoRefetch ? refetchQueryOptions : defaultQueryOptions),
	});
}
