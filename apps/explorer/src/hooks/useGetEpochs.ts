// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { useInfiniteQuery } from '@tanstack/react-query';

export const DEFAULT_EPOCHS_LIMIT = 20;

// Fetch paginated epochs
export function useGetEpochs(limit = DEFAULT_EPOCHS_LIMIT) {
	const client = useSuiClient();

	return useInfiniteQuery(
		['get-epochs-blocks', limit],
		({ pageParam = null }) =>
			client.getEpochs({
				descendingOrder: true,
				cursor: pageParam,
				limit,
			}),
		{
			getNextPageParam: ({ nextCursor, hasNextPage }) => (hasNextPage ? nextCursor : null),
			staleTime: 10 * 1000,
			cacheTime: 24 * 60 * 60 * 1000,
			retry: false,
			keepPreviousData: true,
		},
	);
}
