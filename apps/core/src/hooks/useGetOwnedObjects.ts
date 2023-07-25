// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { type SuiObjectDataFilter } from '@mysten/sui.js';
import { useInfiniteQuery } from '@tanstack/react-query';

const MAX_OBJECTS_PER_REQ = 6;

export function useGetOwnedObjects(
	address?: string | null,
	filter?: SuiObjectDataFilter,
	maxObjectRequests = MAX_OBJECTS_PER_REQ,
) {
	const rpc = useRpcClient();
	return useInfiniteQuery(
		['get-owned-objects', address, filter, maxObjectRequests],
		({ pageParam }) =>
			rpc.getOwnedObjects({
				owner: address!,
				filter,
				options: {
					showType: true,
					showContent: true,
					showDisplay: true,
				},
				limit: maxObjectRequests,
				cursor: pageParam,
			}),
		{
			staleTime: 10 * 1000,
			enabled: !!address,
			getNextPageParam: (lastPage) => (lastPage?.hasNextPage ? lastPage.nextCursor : null),
		},
	);
}
