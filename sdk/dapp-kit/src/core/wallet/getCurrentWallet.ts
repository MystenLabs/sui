// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { StoreState } from '../../walletStore.js';

/**
 * Retrieves the wallet that is currently connected to the dApp, if one exists.
 */
export function getCurrentWallet(state: StoreState) {
	const { currentWallet, connectionStatus, supportedIntents } = state;

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
