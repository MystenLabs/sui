// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useCoinsReFetchingConfig } from '_app/hooks/useCoinsReFetchingConfig';
import { useSuiClientQuery } from '@mysten/dapp-kit';

export function useGetAllBalances(owner: string) {
	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();

	return useSuiClientQuery(
		'getAllBalances',
		{ owner: owner! },
		{
			enabled: !!owner,
			refetchInterval,
			staleTime,
		},
	);
}
