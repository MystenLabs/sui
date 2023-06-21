// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useBackgroundClient } from './useBackgroundClient';
import { type QredoConnectIdentity } from '_src/background/qredo/types';

export function useQredoInfo(
	filter: { qredoID: string } | { identity: QredoConnectIdentity } | null,
) {
	const backgroundClient = useBackgroundClient();
	return useQuery({
		queryKey: ['qredo', 'info', filter],
		queryFn: async () => backgroundClient.getQredoConnectionInfo(filter!),
		enabled: !!filter,
		staleTime: 0,
		refetchInterval: 1000,
		meta: { skipPersistedCache: true },
	});
}
