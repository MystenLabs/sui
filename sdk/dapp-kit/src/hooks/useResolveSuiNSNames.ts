// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ResolvedNameServiceNames } from '@mysten/sui.js/client';
import type { UseQueryOptions, UseQueryResult } from '@tanstack/react-query';

import { useSuiClientQuery } from './useSuiClientQuery.js';

export function useResolveSuiNSName(
	address?: string | null,
	options?: Omit<
		UseQueryOptions<ResolvedNameServiceNames, Error, string | null, unknown[]>,
		'queryFn' | 'queryKey' | 'select'
	>,
): UseQueryResult<string | null, Error> {
	return useSuiClientQuery(
		'resolveNameServiceNames',
		{
			address: address!,
			limit: 1,
		},
		{
			...options,
			refetchOnWindowFocus: false,
			retry: false,
			select: (data) => (data.data.length > 0 ? data.data[0] : null),
			enabled: !!address && options?.enabled !== false,
		},
	);
}
