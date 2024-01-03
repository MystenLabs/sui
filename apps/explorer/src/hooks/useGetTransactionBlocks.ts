// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { keepPreviousData, useInfiniteQuery } from '@tanstack/react-query';

import { type PaginatedTransactionResponse, type TransactionFilter } from '@mysten/sui.js/client';

export const DEFAULT_TRANSACTIONS_LIMIT = 20;

// Fetch transaction blocks
export function useGetTransactionBlocks({
	filter,
	limit = DEFAULT_TRANSACTIONS_LIMIT,
	refetchInterval,
	enabled,
}: {
	filter?: TransactionFilter;
	limit?: number;
	refetchInterval?: number;
	enabled?: boolean;
}) {
	const client = useSuiClient();

	return useInfiniteQuery<PaginatedTransactionResponse>({
		queryKey: ['get-transaction-blocks', filter, limit],
		queryFn: async ({ pageParam }) =>
			await client.queryTransactionBlocks({
				filter,
				cursor: pageParam as string | null,
				order: 'descending',
				limit,
				options: {
					showEffects: true,
					showInput: true,
				},
			}),
		initialPageParam: null,
		getNextPageParam: ({ hasNextPage, nextCursor }) => (hasNextPage ? nextCursor : null),
		staleTime: 10 * 1000,
		retry: false,
		placeholderData: keepPreviousData,
		refetchInterval,
		enabled,
	});
}
