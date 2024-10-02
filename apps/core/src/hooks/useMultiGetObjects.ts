// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { SuiObjectDataOptions, SuiObjectResponse } from '@mysten/sui/client';
import { useQuery, UseQueryOptions } from '@tanstack/react-query';

import { chunkArray } from '../utils/chunkArray';

export function useMultiGetObjects(
	ids: string[],
	options: SuiObjectDataOptions,
	queryOptions?: Omit<UseQueryOptions<SuiObjectResponse[]>, 'queryKey' | 'queryFn'>,
) {
	const client = useSuiClient();
	return useQuery({
		...queryOptions,
		queryKey: ['multiGetObjects', ids],
		queryFn: async () => {
			const responses = await Promise.all(
				chunkArray(ids, 50).map((chunk) =>
					client.multiGetObjects({
						ids: chunk,
						options,
					}),
				),
			);
			return responses.flat();
		},
		enabled: !!ids?.length,
	});
}
