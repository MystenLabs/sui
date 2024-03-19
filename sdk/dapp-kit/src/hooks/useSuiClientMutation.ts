// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { useSuiClientContext } from './useSuiClient.js';
import type { SuiRpcMethods } from './useSuiClientQuery.js';

export type UseSuiClientMutationOptions<T extends keyof SuiRpcMethods> = Omit<
	UseMutationOptions<SuiRpcMethods[T]['result'], Error, SuiRpcMethods[T]['params'], unknown[]>,
	'mutationFn'
>;

export function useSuiClientMutation<T extends keyof SuiRpcMethods>(
	method: T,
	options: UseSuiClientMutationOptions<T> = {},
): UseMutationResult<SuiRpcMethods[T]['result'], Error, SuiRpcMethods[T]['params'], unknown[]> {
	const suiContext = useSuiClientContext();

	return useMutation({
		...options,
		mutationFn: async (params) => {
			return await suiContext.client[method](params as never);
		},
	});
}
