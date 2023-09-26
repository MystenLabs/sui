// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { useInfiniteQuery } from '@tanstack/react-query';

export const DEFAULT_CHECKPOINTS_LIMIT = 20;

// Fetch transaction blocks
export function useGetCheckpoints(cursor?: string, limit = DEFAULT_CHECKPOINTS_LIMIT) {
	const client = useSuiClient();

	return useInfiniteQuery(
		['get-checkpoints', limit, cursor],
		async ({ pageParam }) =>
			await client.getCheckpoints({
				descendingOrder: true,
				cursor: pageParam ?? cursor,
				limit,
			}),
		{
			getNextPageParam: (lastPage) => (lastPage?.hasNextPage ? lastPage.nextCursor : false),
			staleTime: 10 * 1000,
			cacheTime: 24 * 60 * 60 * 1000,
			retry: false,
			keepPreviousData: true,
		},
	);
}
