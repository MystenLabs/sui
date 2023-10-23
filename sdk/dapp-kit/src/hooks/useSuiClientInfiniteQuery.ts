// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui.js/client';
import type { UseInfiniteQueryOptions } from '@tanstack/react-query';
import { useInfiniteQuery } from '@tanstack/react-query';

import type { PartialBy } from '../types/utilityTypes.js';
import { useSuiClientContext } from './useSuiClient.js';

interface PaginatedResult {
	data?: unknown;
	nextCursor?: unknown;
	hasNextPage: boolean;
}

export type SuiRpcPaginatedMethodName = {
	[K in keyof SuiClient]: SuiClient[K] extends (input: any) => Promise<PaginatedResult> ? K : never;
}[keyof SuiClient];

export type SuiRpcPaginatedMethods = {
	[K in SuiRpcPaginatedMethodName]: SuiClient[K] extends (input: infer P) => Promise<{
		data?: infer R;
		nextCursor?: infer Cursor | null;
		hasNextPage: boolean;
	}>
		? {
				name: K;
				result: {
					data?: R;
					nextCursor?: Cursor | null;
					hasNextPage: boolean;
				};
				params: P;
				cursor: Cursor;
		  }
		: never;
};

export type UseSuiClientInfiniteQueryOptions<
	T extends keyof SuiRpcPaginatedMethods,
	TData,
> = PartialBy<
	Omit<
		UseInfiniteQueryOptions<
			SuiRpcPaginatedMethods[T]['result'],
			Error,
			TData,
			SuiRpcPaginatedMethods[T]['result'],
			unknown[]
		>,
		'queryFn' | 'initialPageParam' | 'getNextPageParam'
	>,
	'queryKey'
>;

export function useSuiClientInfiniteQuery<
	T extends keyof SuiRpcPaginatedMethods,
	TData = SuiRpcPaginatedMethods[T]['result'],
>(
	method: T,
	params: SuiRpcPaginatedMethods[T]['params'],
	{
		queryKey = [],
		enabled = !!params,
		...options
	}: UseSuiClientInfiniteQueryOptions<T, TData> = {},
) {
	const suiContext = useSuiClientContext();

	return useInfiniteQuery({
		...options,
		initialPageParam: null,
		queryKey: [suiContext.network, method, params, ...queryKey],
		enabled,
		queryFn: () => suiContext.client[method](params as never),
		getNextPageParam: ({ hasNextPage, nextCursor }) => (hasNextPage ? nextCursor : null),
	});
}
