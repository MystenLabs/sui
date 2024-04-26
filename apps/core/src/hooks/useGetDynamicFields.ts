// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { DynamicFieldPage } from '@mysten/sui/client';
import { normalizeSuiAddress } from '@mysten/sui/utils';
import { useInfiniteQuery } from '@tanstack/react-query';

const MAX_PAGE_SIZE = 10;

export function useGetDynamicFields(parentId: string, maxPageSize = MAX_PAGE_SIZE) {
	const client = useSuiClient();
	return useInfiniteQuery<DynamicFieldPage>({
		queryKey: ['dynamic-fields', { maxPageSize, parentId }],
		queryFn: ({ pageParam = null }) =>
			client.getDynamicFields({
				parentId: normalizeSuiAddress(parentId),
				cursor: pageParam as string | null,
				limit: maxPageSize,
			}),
		enabled: !!parentId,
		initialPageParam: null,
		getNextPageParam: ({ nextCursor, hasNextPage }) => (hasNextPage ? nextCursor : null),
	});
}
