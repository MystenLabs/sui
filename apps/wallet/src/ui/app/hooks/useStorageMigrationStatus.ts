// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';

import { useBackgroundClient } from './useBackgroundClient';

export function useStorageMigrationStatus() {
	const [enabled, setEnabled] = useState(true);
	const backgroundClient = useBackgroundClient();
	const response = useQuery({
		queryKey: ['storage migration status'],
		queryFn: () => backgroundClient.getStorageMigrationStatus(),
		refetchInterval: 1000,
		enabled,
		meta: { skipPersistedCache: true },
	});
	useEffect(() => {
		if (response.data === 'ready') {
			setEnabled(false);
		}
	}, [response.data]);
	return response;
}
