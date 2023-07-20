// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { type SuiMoveNormalizedStruct } from '@mysten/sui.js';
import { normalizeSuiObjectId } from '@mysten/sui.js/utils';
import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

type GetNormalizedMoveStructOptions = {
	packageId: string;
	module: string;
	struct: string;
} & Pick<UseQueryOptions<SuiMoveNormalizedStruct, unknown>, 'onSuccess' | 'onError'>;

export function useGetNormalizedMoveStruct(options: GetNormalizedMoveStructOptions) {
	const { packageId, module, struct, ...useQueryOptions } = options;
	const rpc = useRpcClient();
	return useQuery({
		queryKey: ['normalized-struct', packageId, module, struct],
		queryFn: () =>
			rpc.getNormalizedMoveStruct({
				package: normalizeSuiObjectId(packageId),
				module,
				struct,
			}),
		enabled: !!packageId && !!module && !!struct,
		...useQueryOptions,
	});
}
