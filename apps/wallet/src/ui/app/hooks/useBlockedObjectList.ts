// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { normalizeStructTag } from '@mysten/sui/utils';
import { useQuery } from '@tanstack/react-query';

import { useAppsBackend } from '../../../../../core';

export function useBlockedObjectList() {
	const { request } = useAppsBackend();
	return useQuery({
		queryKey: ['apps-backend', 'guardian', 'object-list'],
		queryFn: () => request<{ blocklist: string[] }>('guardian/object-list'),
		select: (data) => data?.blocklist.map(normalizeStructTag) ?? [],
	});
}
