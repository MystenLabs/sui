// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { parseStructTag } from '@mysten/sui/utils';
import { useQuery } from '@tanstack/react-query';

import { useAppsBackend } from '../../../../../core';

export function useBlockedObjectList() {
	const { request } = useAppsBackend();
	return useQuery({
		queryKey: ['apps-backend', 'blocklist'],
		queryFn: () => request<{ blocklist: string[] }>('guardian/object-list'),
		select: (data) =>
			data?.blocklist?.map((list) => {
				const { address } = parseStructTag(list);
				return address;
			}) ?? [],
	});
}
