// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import type { DelegatedStake } from '@mysten/sui.js/client';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

export function useGetDelegatedStake(address: string): UseQueryResult<DelegatedStake[], Error> {
	const client = useSuiClient();
	return useQuery({
		queryKey: ['validator', address],
		queryFn: () => client.getStakes({ owner: address }),
		staleTime: 10 * 1000,
		refetchInterval: 30 * 1000,
	});
}
