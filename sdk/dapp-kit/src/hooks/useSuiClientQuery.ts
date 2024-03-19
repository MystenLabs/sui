// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui.js/client';
import type { UseQueryOptions, UseQueryResult } from '@tanstack/react-query';
import { useQuery } from '@tanstack/react-query';

import type { PartialBy } from '../types/utilityTypes.js';
import { useSuiClientContext } from './useSuiClient.js';

export type SuiRpcMethodName = {
	[K in keyof SuiClient]: SuiClient[K] extends ((input: any) => Promise<any>) | (() => Promise<any>)
		? K
		: never;
}[keyof SuiClient];

export type SuiRpcMethods = {
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
				params: undefined | object;
		  }
		: never;
};

export type UseSuiClientQueryOptions<T extends keyof SuiRpcMethods, TData> = PartialBy<
	Omit<UseQueryOptions<SuiRpcMethods[T]['result'], Error, TData, unknown[]>, 'queryFn'>,
	'queryKey'
>;

export function useSuiClientQuery<
	T extends keyof SuiRpcMethods,
	TData = SuiRpcMethods[T]['result'],
>(
	...args: undefined extends SuiRpcMethods[T]['params']
		? [method: T, params?: SuiRpcMethods[T]['params'], options?: UseSuiClientQueryOptions<T, TData>]
		: [method: T, params: SuiRpcMethods[T]['params'], options?: UseSuiClientQueryOptions<T, TData>]
): UseQueryResult<TData, Error> {
	const [method, params, { queryKey = [], ...options } = {}] = args as [
		method: T,
		params?: SuiRpcMethods[T]['params'],
		options?: UseSuiClientQueryOptions<T, TData>,
	];

	const suiContext = useSuiClientContext();

	return useQuery({
		...options,
		queryKey: [suiContext.network, method, params, ...queryKey],
		queryFn: async () => {
			return await suiContext.client[method](params as never);
		},
	});
}
