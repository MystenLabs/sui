// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseQueryResult } from '@tanstack/react-query';
import { useQueries } from '@tanstack/react-query';

import { useSuiClientContext } from './useSuiClient.js';
import type { SuiRpcMethods, UseSuiClientQueryOptions } from './useSuiClientQuery.js';

type SuiClientQueryOptions = SuiRpcMethods[keyof SuiRpcMethods] extends infer Method
	? Method extends {
			name: infer M extends keyof SuiRpcMethods;
			params?: infer P;
	  }
		? undefined extends P
			? {
					method: M;
					params?: P;
					options?: UseSuiClientQueryOptions<M, unknown>;
			  }
			: {
					method: M;
					params: P;
					options?: UseSuiClientQueryOptions<M, unknown>;
			  }
		: never
	: never;

export type UseSuiClientQueriesResults<Args extends readonly SuiClientQueryOptions[]> = {
	-readonly [K in keyof Args]: Args[K] extends {
		method: infer M extends keyof SuiRpcMethods;
		readonly options?:
			| {
					select?: (...args: any[]) => infer R;
			  }
			| object;
	}
		? UseQueryResult<unknown extends R ? SuiRpcMethods[M]['result'] : R>
		: never;
};

export function useSuiClientQueries<
	const Queries extends readonly SuiClientQueryOptions[],
	Results = UseSuiClientQueriesResults<Queries>,
>({
	queries,
	combine,
}: {
	queries: Queries;
	combine?: (results: UseSuiClientQueriesResults<Queries>) => Results;
}): Results {
	const suiContext = useSuiClientContext();

	return useQueries({
		combine: combine as never,
		queries: queries.map((query) => {
			const { method, params, options: { queryKey = [], ...restOptions } = {} } = query;

			return {
				...restOptions,
				queryKey: [suiContext.network, method, params, ...queryKey],
				queryFn: async () => {
					return await suiContext.client[method](params as never);
				},
			};
		}) as [],
	});
}
