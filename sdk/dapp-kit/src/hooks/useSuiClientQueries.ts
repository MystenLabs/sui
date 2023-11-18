// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClientContext } from '@mysten/dapp-kit';
import type { SuiClient } from '@mysten/sui.js/client';
import type { UseQueryOptions, UseQueryResult } from '@tanstack/react-query';
import { useQueries } from '@tanstack/react-query';

import type { SuiRpcMethods, UseSuiClientQueryOptions } from './useSuiClientQuery.js';

export function useSuiClientQueries<
	T extends keyof SuiRpcMethods,
	TData = SuiRpcMethods[T]['result'],
>(
	...args: undefined extends SuiRpcMethods[T]['params']
		? [
				method: T,
				params?: SuiRpcMethods[T]['params'],
				options?: UseSuiClientQueryOptions<T, TData>,
		  ][]
		: [
				method: T,
				params: SuiRpcMethods[T]['params'],
				options?: UseSuiClientQueryOptions<T, TData>,
		  ][]
): UseQueryResult<TData, Error>[] {
	const queries = args;

	const suiContext = useSuiClientContext();

	return useQueries({
		queries: queries.map((query) => {
			const [method, params, { queryKey = [], ...options } = {}] = query;

			return {
				...options,
				queryKey: [suiContext.network, method, params, ...queryKey],
				queryFn: async () => {
					return await suiContext.client[method](params as never);
				},
			};
		}),
	});
}
