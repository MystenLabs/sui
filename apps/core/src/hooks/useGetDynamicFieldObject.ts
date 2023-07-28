// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DynamicFieldName } from '@mysten/sui.js/client';
import { useRpcClient } from '../api/RpcClientContext';
import { useQuery } from '@tanstack/react-query';

export function useGetDynamicFieldObject(parentId: string, name: DynamicFieldName) {
	const rpc = useRpcClient();
	return useQuery({
		queryKey: ['dynamic-fields-object', parentId, name],
		queryFn: () =>
			rpc.getDynamicFieldObject({
				parentId,
				name,
			}),
		enabled: !!parentId && !!name,
	});
}
