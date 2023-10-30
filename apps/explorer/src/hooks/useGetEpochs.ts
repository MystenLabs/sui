// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { type EpochPage } from '@mysten/sui.js/client';
import { keepPreviousData, useInfiniteQuery } from '@tanstack/react-query';

export const DEFAULT_EPOCHS_LIMIT = 20;

// Fetch paginated epochs
export function useGetEpochs(limit = DEFAULT_EPOCHS_LIMIT) {
	const client = useSuiClient();

	return useInfiniteQuery<EpochPage>({
		queryKey: ['get-epochs-blocks', limit],
		queryFn: ({ pageParam }) =>
			client.getEpochs({
				descendingOrder: true,
				cursor: pageParam as string | null,
				limit,
			}),
		initialPageParam: null,
		getNextPageParam: ({ nextCursor, hasNextPage }) => (hasNextPage ? nextCursor : null),
		staleTime: 10 * 1000,
		gcTime: 24 * 60 * 60 * 1000,
		retry: false,
		placeholderData: keepPreviousData,
	});
}
