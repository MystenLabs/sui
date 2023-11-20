// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseQueryResult } from '@tanstack/react-query';
import { useQueries } from '@tanstack/react-query';

import { useSuiClientContext } from './useSuiClient.js';
import type { SuiRpcMethods, UseSuiClientQueryOptions } from './useSuiClientQuery.js';

type BaseSuiClientQueriesArgs = {
	method: keyof SuiRpcMethods;
	params?: object;
	options?: UseSuiClientQueryOptions<keyof SuiRpcMethods, unknown>;
}[];

type TypedSuiClientQueriesArgs<Args extends BaseSuiClientQueriesArgs> = {
	[K in keyof Args]: Args[K] extends { method: infer T extends keyof SuiRpcMethods }
		? undefined extends SuiRpcMethods[T]['params']
			? {
					method: T;
					params?: SuiRpcMethods[T]['params'];
					options?: UseSuiClientQueryOptions<T, SuiRpcMethods[T]['result']>;
			  }
			: {
					method: T;
					params: SuiRpcMethods[T]['params'];
					options?: UseSuiClientQueryOptions<T, SuiRpcMethods[T]['result']>;
			  }
		: never;
};

export function useSuiClientQueries<
	Args extends BaseSuiClientQueriesArgs,
	TypedArgs extends TypedSuiClientQueriesArgs<Args>,
>(
	...args: Args extends TypedArgs ? Args : TypedArgs
): {
	[K in keyof Args]: Args[K] extends { method: infer T extends keyof SuiRpcMethods }
		? UseQueryResult<SuiRpcMethods[T]['result'], Error>
		: never;
} {
	const queries = args;

	const suiContext = useSuiClientContext();

	return useQueries({
		queries: queries.map((query) => {
			const { method, params, options: { queryKey = [], ...restOptions } = {} } = query;

			return {
				...restOptions,
				queryKey: [suiContext.network, method, params, ...queryKey],
				queryFn: async () => {
					return await suiContext.client[method](params as never);
				},
			};
		}),
	});
}
