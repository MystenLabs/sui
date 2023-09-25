// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { PaginatedCoins } from '@mysten/sui.js/client';
import { useInfiniteQuery, UseInfiniteQueryResult } from '@tanstack/react-query';

const MAX_COINS_PER_REQUEST = 10;

export function useGetCoins(
	coinType: string,
	address?: string | null,
	maxCoinsPerRequest = MAX_COINS_PER_REQUEST,
): UseInfiniteQueryResult<PaginatedCoins> {
	const client = useSuiClient();
	return useInfiniteQuery(
		['get-coins', address, coinType, maxCoinsPerRequest],
		({ pageParam }) =>
			client.getCoins({
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
