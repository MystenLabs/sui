// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { useInfiniteQuery } from '@tanstack/react-query';
import { normalizeSuiAddress } from '@mysten/sui.js/utils';

const MAX_PAGE_SIZE = 10;

export function useGetDynamicFields(parentId: string, maxPageSize = MAX_PAGE_SIZE) {
	const rpc = useRpcClient();
	return useInfiniteQuery(
		['dynamic-fields', parentId],
		({ pageParam = null }) =>
			rpc.getDynamicFields({
				parentId: normalizeSuiAddress(parentId),
				cursor: pageParam,
				limit: maxPageSize,
			}),
		{
			enabled: !!parentId,
			getNextPageParam: ({ nextCursor, hasNextPage }) => (hasNextPage ? nextCursor : null),
		},
	);
}
