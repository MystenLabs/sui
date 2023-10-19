// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { type CheckpointPage } from '@mysten/sui.js/client';
import { keepPreviousData, useInfiniteQuery } from '@tanstack/react-query';

export const DEFAULT_CHECKPOINTS_LIMIT = 20;

// Fetch transaction blocks
export function useGetCheckpoints(cursor?: string, limit = DEFAULT_CHECKPOINTS_LIMIT) {
	const client = useSuiClient();

	return useInfiniteQuery<CheckpointPage>({
		queryKey: ['get-checkpoints', limit, cursor],
		queryFn: async ({ pageParam }) =>
			await client.getCheckpoints({
				descendingOrder: true,
				cursor: (pageParam as string | null) ?? cursor,
				limit,
			}),
		initialPageParam: null,
		getNextPageParam: ({ hasNextPage, nextCursor }) => (hasNextPage ? nextCursor : null),
		staleTime: 10 * 1000,
		gcTime: 24 * 60 * 60 * 1000,
		retry: false,
		placeholderData: keepPreviousData,
	});
}
