// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { useQuery } from '@tanstack/react-query';

import { accountsQueryKey } from '../helpers/query-client-keys';
import { useBackgroundClient } from './useBackgroundClient';

export function useAccounts() {
	const backgroundClient = useBackgroundClient();
	return useQuery({
		queryKey: accountsQueryKey,
		queryFn: () => backgroundClient.getStoredEntities<SerializedUIAccount>('accounts'),
		gcTime: 30 * 1000,
		staleTime: 15 * 1000,
		refetchInterval: 30 * 1000,
		meta: { skipPersistedCache: true },
	});
}
