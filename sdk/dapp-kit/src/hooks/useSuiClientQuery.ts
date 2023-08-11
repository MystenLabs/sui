// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseQueryOptions } from '@tanstack/react-query';
import { useQuery } from '@tanstack/react-query';
import { useSuiClientContext } from './useSuiClient.js';
import type { SuiClient } from '@mysten/sui.js/client';

type SuiRpcMethodName = {
	[K in keyof SuiClient]: SuiClient[K] extends ((input: any) => Promise<any>) | (() => Promise<any>)
		? K
		: never;
}[keyof SuiClient];

type Methods = {
	[K in SuiRpcMethodName]: SuiClient[K] extends (input: infer P) => Promise<infer R>
		? {
				name: K;
				result: R;
				params: P;
		  }
		: SuiClient[K] extends () => Promise<infer R>
		? {
				name: K;
				result: R;
				params: undefined;
		  }
		: never;
};

export type UseSuiClientQueryOptions<T extends keyof Methods> = Omit<
	UseQueryOptions<Methods[T]['result'], unknown, Methods[T]['result'], unknown[]>,
	'queryFn'
>;

export function useSuiClientQuery<T extends keyof Methods>(
	{
		method,
		params,
	}: {
		method: T;
		params: Methods[T]['params'];
	},
	{
		queryKey,

		enabled = !!params,
		...options
	}: UseSuiClientQueryOptions<T> = {},
) {
	const suiContext = useSuiClientContext();

	return useQuery({
		...options,
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: suiContext.queryKey(queryKey ?? [method, params]),
		enabled,
		queryFn: async () => {
			return await suiContext.client[method](params as never);
		},
	});
}
