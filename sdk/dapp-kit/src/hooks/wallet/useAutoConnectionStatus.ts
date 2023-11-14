// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutationState } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the status for the initial wallet auto-connection process.
 */
export function useAutoConnectionStatus(): 'disabled' | 'idle' | 'attempted' {
	const autoConnectEnabled = useWalletStore((state) => state.autoConnectEnabled);
	const hasLastConnectedWallet = useWalletStore((state) => !!state.lastConnectedWalletName);
	const [mutationState] = useMutationState({
		filters: { mutationKey: walletMutationKeys.autoconnectWallet() },
	});

	if (!autoConnectEnabled) {
		return 'disabled';
	}

	if (mutationState) {
		return mutationState.status === 'error' || mutationState.status === 'success'
			? 'attempted'
			: 'idle';
	}

	return hasLastConnectedWallet ? 'idle' : 'attempted';
}
