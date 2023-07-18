// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { useBackgroundClient } from '../useBackgroundClient';
import { type AccountSourceSerializedUI } from '_src/background/account-sources/AccountSource';

export const accountSourcesQueryKey = ['background', 'client', 'account', 'sources'] as const;

export function useAccountSources() {
	const backgroundClient = useBackgroundClient();
	return useQuery({
		queryKey: accountSourcesQueryKey,
		queryFn: () =>
			backgroundClient.getStoredEntities<AccountSourceSerializedUI>('account-source-entity'),
		cacheTime: 30 * 1000,
		staleTime: 15 * 1000,
		refetchInterval: 5 * 1000,
		meta: { skipPersistedCache: true },
	});
}
