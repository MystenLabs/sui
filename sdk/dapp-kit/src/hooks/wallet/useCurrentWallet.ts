// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the wallet that is currently connected to the dApp, if one exists.
 */
export function useCurrentWallet() {
	const currentWallet = useWalletStore((state) => state.currentWallet);
	const connectionStatus = useWalletStore((state) => state.connectionStatus);
	const supportedIntents = useWalletStore((state) => state.supportedIntents);

	switch (connectionStatus) {
		case 'connecting':
			return {
				connectionStatus,
				currentWallet: null,
				isDisconnected: false,
				isConnecting: true,
				isConnected: false,
				supportedIntents: [],
			} as const;
		case 'disconnected':
			return {
				connectionStatus,
				currentWallet: null,
				isDisconnected: true,
				isConnecting: false,
				isConnected: false,
				supportedIntents: [],
			} as const;
		case 'connected': {
			return {
				connectionStatus,
				currentWallet: currentWallet!,
				isDisconnected: false,
				isConnecting: false,
				isConnected: true,
				supportedIntents,
			} as const;
		}
	}
}
