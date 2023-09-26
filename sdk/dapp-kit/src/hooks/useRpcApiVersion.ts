// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseSuiClientQueryOptions } from './useSuiClientQuery.js';
import { useSuiClientQuery } from './useSuiClientQuery.js';

export function useRpcApiVersion(options?: UseSuiClientQueryOptions<'getRpcApiVersion'>) {
	return useSuiClientQuery(
		{
			method: 'getRpcApiVersion',
			params: {},
		},
		options,
	);
}
