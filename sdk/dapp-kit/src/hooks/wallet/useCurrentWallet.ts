// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the wallet that is currently connected to the dApp, if one exists.
 */
export function useCurrentWallet() {
	const currentWallet = useWalletStore((state) => state.currentWallet);
	const connectionStatus = useWalletStore((state) => state.connectionStatus);

	switch (connectionStatus) {
		case 'connecting':
			return {
				connectionStatus,
				currentWallet: null,
				isDisconnected: false,
				isConnecting: true,
				isReconnecting: false,
				isConnected: false,
			} as const;
		case 'reconnecting':
			return {
				connectionStatus,
				currentWallet: null,
				isDisconnected: false,
				isReconnecting: true,
				isConnecting: false,
				isConnected: false,
			} as const;
		case 'disconnected':
			return {
				connectionStatus,
				currentWallet: null,
				isDisconnected: true,
				isConnecting: false,
				isReconnecting: true,
				isConnected: false,
			} as const;
		case 'connected': {
			return {
				connectionStatus,
				currentWallet: currentWallet!,
				isDisconnected: false,
				isConnecting: false,
				isReconnecting: false,
				isConnected: true,
			} as const;
		}
	}
}
