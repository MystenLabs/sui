// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useIsFetching, useQueryClient } from '@tanstack/react-query';

import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the status for the initial wallet auto-connection process.
 */
export function useAutoConnectionStatus(): 'idle' | 'attempted' {
	const queryClient = useQueryClient();
	const queryCache = queryClient.getQueryCache();
	const hasLastConnectedWallet = useWalletStore((state) => !!state.lastConnectedWalletName);

	// Subscribe to isFetching to trigger a re-render when the query state changes:
	useIsFetching({
		queryKey: ['@mysten/dapp-kit', 'autoconnect'],
	});

	const [queryState] = queryCache.findAll({ queryKey: ['@mysten/dapp-kit', 'autoconnect'] });

	if (queryState) {
		return queryState.state.status === 'error' || queryState.state.status === 'success'
			? 'attempted'
			: 'idle';
	}

	return hasLastConnectedWallet ? 'idle' : 'attempted';
}
