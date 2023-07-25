// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

export function useNormalizedMoveModule(packageId?: string | null, moduleName?: string | null) {
	const rpc = useRpcClient();
	return useQuery({
		queryKey: ['normalized-module', packageId, moduleName],
		queryFn: async () =>
			await rpc.getNormalizedMoveModule({
				package: packageId!,
				module: moduleName!,
			}),
		enabled: !!(packageId && moduleName),
	});
}
