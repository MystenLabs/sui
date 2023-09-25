// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
/* eslint-disable @tanstack/query/exhaustive-deps */

import { PaginatedObjectsResponse } from '@mysten/sui.js/client';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '../context/RpcClientContext';
import { TANSTACK_OWNED_OBJECTS_KEY } from '../utils/constants';
import { parseObjectDisplays } from '../utils/utils';

export function useOwnedObjects({
	address,
	cursor = undefined,
	limit = 50,
}: {
	address: string;
	cursor?: string;
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
				display: displays[item.data?.objectId!] || {},
				type:
					item.data?.type ??
					(item?.data?.content?.dataType === 'package' ? 'package' : item?.data?.content?.type) ??
					'',
				isLocked: false,
				objectId: item.data?.objectId,
			}));
		},
	});
}
