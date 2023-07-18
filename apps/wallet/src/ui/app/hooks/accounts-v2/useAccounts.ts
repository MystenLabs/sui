// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { useBackgroundClient } from '../useBackgroundClient';
import { type SerializedUIAccount } from '_src/background/accounts/Account';

export const accountsQueryKey = ['background', 'client', 'accounts'] as const;

export function useAccounts() {
	const backgroundClient = useBackgroundClient();
	return useQuery({
		queryKey: accountsQueryKey,
		queryFn: () => backgroundClient.getStoredEntities<SerializedUIAccount>('account-entity'),
		cacheTime: 30 * 1000,
		staleTime: 15 * 1000,
		refetchInterval: 5 * 1000,
		meta: { skipPersistedCache: true },
	});
}
