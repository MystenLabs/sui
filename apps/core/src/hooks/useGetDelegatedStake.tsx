// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import type { DelegatedStake } from '@mysten/sui.js/client';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

type UseGetDelegatedStakeOptions = {
	autoRefetch?: boolean;
};

export function useGetDelegatedStake(
	address: string,
	options?: UseGetDelegatedStakeOptions,
): UseQueryResult<DelegatedStake[], Error> {
	const client = useSuiClient();
	const { autoRefetch = false } = options || {};

	// Generalized query options
	const defaultQueryOptions = {
		staleTime: 10 * 1000,
		refetchInterval: 30 * 1000,
	};

	const refetchQueryOptions = {
		staleTime: Infinity, // not stale until data changes at the query key
		refetchInterval: false, // no automatic refetching
	} as const;

	return useQuery({
		queryKey: ['validator', address],
		queryFn: () => client.getStakes({ owner: address }),
		...(autoRefetch ? refetchQueryOptions : defaultQueryOptions),
	});
}
