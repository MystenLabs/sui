// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectId, SuiObjectDataOptions } from '@mysten/sui.js';
import { useRpcClient } from '../api/RpcClientContext';
import { useQuery } from '@tanstack/react-query';
import { chunkArray } from '../utils/chunkArray';

export function useMultiGetObjects(ids: ObjectId[], options: SuiObjectDataOptions) {
	const rpc = useRpcClient();
	return useQuery({
		queryKey: ['multiGetObjects', ids],
		queryFn: async () => {
			const responses = await Promise.all(
				chunkArray(ids, 50).map((chunk) =>
					rpc.multiGetObjects({
						ids: chunk,
						options,
					}),
				),
			);
			return responses.flat();
		},
		enabled: !!ids?.length,
	});
}
