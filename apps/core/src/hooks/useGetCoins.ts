// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { useInfiniteQuery } from '@tanstack/react-query';

const MAX_COINS_PER_REQUEST = 10;

export function useGetCoins(
	coinType: string,
	address?: string | null,
	maxCoinsPerRequest = MAX_COINS_PER_REQUEST,
) {
	const rpc = useRpcClient();
	return useInfiniteQuery(
		['get-coins', address, coinType, maxCoinsPerRequest],
		({ pageParam }) =>
			rpc.getCoins({
				owner: address!,
				coinType,
				cursor: pageParam ? pageParam.cursor : null,
				limit: maxCoinsPerRequest,
			}),
		{
			getNextPageParam: ({ hasNextPage, nextCursor }) =>
				hasNextPage
					? {
							cursor: nextCursor,
					  }
					: false,
			enabled: !!address,
		},
	);
}
