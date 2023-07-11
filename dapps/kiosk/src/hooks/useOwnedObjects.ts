// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
/* eslint-disable @tanstack/query/exhaustive-deps */

import { useQuery } from '@tanstack/react-query';
import { useRpc } from '../context/RpcClientContext';
import { PaginatedObjectsResponse, SuiAddress, getObjectId, getObjectType } from '@mysten/sui.js';
import { parseObjectDisplays } from '../utils/utils';
import { TANSTACK_OWNED_OBJECTS_KEY } from '../utils/constants';

export function useOwnedObjects({
	address,
	cursor = undefined,
	limit = 50,
}: {
	address: SuiAddress;
	cursor?: SuiAddress;
	limit?: number;
}) {
	const provider = useRpc();

	return useQuery({
		queryKey: [TANSTACK_OWNED_OBJECTS_KEY, address],
		queryFn: async () => {
			if (!address) return [];
			const { data }: PaginatedObjectsResponse = await provider.getOwnedObjects({
				owner: address,
				options: {
					showDisplay: true,
					showType: true,
				},
				cursor,
				limit,
			});

			if (!data) return;

			const displays = parseObjectDisplays(data);

			// Simple mapping to OwnedObject style.
			return data.map((item) => ({
				display: displays[getObjectId(item)] || {},
				type: getObjectType(item) || '',
				isLocked: false,
				objectId: getObjectId(item),
			}));
		},
	});
}
