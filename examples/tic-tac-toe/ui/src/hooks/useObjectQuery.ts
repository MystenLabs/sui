// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClientContext, useSuiClientQuery, UseSuiClientQueryOptions } from '@mysten/dapp-kit';
import { GetObjectParams, SuiObjectResponse } from '@mysten/sui/client';
import { useQueryClient, UseQueryResult } from '@tanstack/react-query';

export type UseObjectQueryOptions = UseSuiClientQueryOptions<'getObject', SuiObjectResponse>;
export type UseObjectQueryResponse = UseQueryResult<SuiObjectResponse, Error>;
export type InvalidateUseObjectQuery = () => void;

/**
 * Fetches an object, returning the response from RPC and a callback
 * to invalidate it.
 */
export function useObjectQuery(
	params: GetObjectParams,
	options?: UseObjectQueryOptions,
): [UseObjectQueryResponse, InvalidateUseObjectQuery] {
	const ctx = useSuiClientContext();
	const client = useQueryClient();
	const response = useSuiClientQuery('getObject', params, options);

	const invalidate = async () => {
		await client.invalidateQueries({
			queryKey: [ctx.network, 'getObject', params],
		});
	};

	return [response, invalidate];
}
