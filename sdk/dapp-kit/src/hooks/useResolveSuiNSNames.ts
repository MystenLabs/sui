// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ResolvedNameServiceNames } from '@mysten/sui.js/client';
import type { UseQueryOptions } from '@tanstack/react-query';

import { useSuiClientQuery } from './useSuiClientQuery.js';

export function useResolveSuiNSName(
	address?: string | null,
	options?: Omit<
		UseQueryOptions<ResolvedNameServiceNames, Error, ResolvedNameServiceNames, unknown[]>,
		'queryFn'
	>,
) {
	const { data, ...rest } = useSuiClientQuery(
		'resolveNameServiceNames',
		{
			address: address!,
			limit: 1,
		},
		{
			refetchOnWindowFocus: false,
			retry: false,
			...options,
			enabled: !!address && options?.enabled !== false,
		},
	);

	return { data: data?.data?.[0] ?? null, ...rest };
}
