// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutationState } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the status for the initial wallet auto-connection process.
 */
export function useAutoConnectionStatus(): 'idle' | 'attempted' {
	const hasLastConnectedWallet = useWalletStore((state) => !!state.lastConnectedWalletName);

	const [mutationState] = useMutationState({
		filters: { mutationKey: walletMutationKeys.connectWallet() },
	});

	if (mutationState) {
		return mutationState.status === 'error' || mutationState.status === 'success'
			? 'attempted'
			: 'idle';
	}

	return hasLastConnectedWallet ? 'idle' : 'attempted';
}
