// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui.js/client';
import type { UseInfiniteQueryOptions } from '@tanstack/react-query';
import { useInfiniteQuery } from '@tanstack/react-query';

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
				result: R;
				params: P;
				cursor: Cursor;
		  }
		: never;
};

export type UseSuiClientInfiniteQueryOptions<T extends keyof SuiRpcPaginatedMethods> = Omit<
	UseInfiniteQueryOptions<
		SuiRpcPaginatedMethods[T]['result'],
		Error,
		SuiRpcPaginatedMethods[T]['result'],
		SuiRpcPaginatedMethods[T]['result'],
		unknown[]
	>,
	'queryFn'
>;

export function useSuiClientInfiniteQuery<T extends keyof SuiRpcPaginatedMethods>(
	method: T,
	params: SuiRpcPaginatedMethods[T]['params'],
	{ queryKey = [], enabled = !!params, ...options }: UseSuiClientInfiniteQueryOptions<T> = {},
) {
	const suiContext = useSuiClientContext();

	return useInfiniteQuery({
		...options,
		queryKey: [suiContext.network, method, params, ...queryKey],
		enabled,
		queryFn: async () => {
			return await suiContext.client[method](params as never);
		},
		getNextPageParam: (lastPage) => {
			return (lastPage as PaginatedResult).nextCursor ?? null;
		},
	});
}
