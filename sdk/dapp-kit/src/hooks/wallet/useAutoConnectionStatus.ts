// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutationState } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { useCurrentWallet } from './useCurrentWallet.js';
import { useWallets } from './useWallets.js';
import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the status for the initial wallet auto-connection process.
 */
export function useAutoConnectionStatus(): 'disabled' | 'idle' | 'attempted' {
	const autoConnectEnabled = useWalletStore((state) => state.autoConnectEnabled);
	const lastConnectedWalletName = useWalletStore((state) => state.lastConnectedWalletName);
	const lastConnectedAccountAddress = useWalletStore((state) => state.lastConnectedAccountAddress);
	const wallets = useWallets();
	const { isDisconnected } = useCurrentWallet();

	const [mutationState] = useMutationState({
		filters: {
			mutationKey: walletMutationKeys.autoconnectWallet(),
			predicate: ({ state: { variables } }) => {
				return (
					variables &&
					variables.autoConnectEnabled === autoConnectEnabled &&
					variables.lastConnectedAccountAddress === lastConnectedAccountAddress &&
					variables.lastConnectedWalletName === lastConnectedWalletName &&
					variables.isDisconnected === isDisconnected &&
					variables.wallets === wallets
				);
			},
		},
	});

	if (!autoConnectEnabled) {
		return 'disabled';
	}

	if (mutationState) {
		return mutationState.status === 'error' || mutationState.status === 'success'
			? 'attempted'
			: 'idle';
	}

	return lastConnectedWalletName ? 'idle' : 'attempted';
}
