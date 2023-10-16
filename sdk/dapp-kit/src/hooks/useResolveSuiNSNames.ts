// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClientQuery } from './useSuiClientQuery.js';

export function useResolveSuiNSName(address?: string | null) {
	const { data, ...rest } = useSuiClientQuery(
		'resolveNameServiceNames',
		{
			address: address!,
			limit: 1,
		},
		{
			enabled: !!address,
			refetchOnWindowFocus: false,
			retry: false,
		},
	);

	return { data: data?.data?.[0] ?? null, ...rest };
}
