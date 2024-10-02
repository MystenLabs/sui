// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useBackgroundClient } from './useBackgroundClient';

export function useQredoInfo(qredoID: string | null) {
	const backgroundClient = useBackgroundClient();
	return useQuery({
		queryKey: ['qredo', 'info', qredoID],
		queryFn: async () => backgroundClient.getQredoConnectionInfo(qredoID!),
		enabled: !!qredoID,
		staleTime: 0,
		refetchInterval: 1000,
		meta: { skipPersistedCache: true },
	});
}
