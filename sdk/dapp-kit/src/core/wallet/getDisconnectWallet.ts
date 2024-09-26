// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { WalletNotConnectedError } from '../../errors/walletErrors.js';
import type { StoreState } from '../../walletStore.js';
import { getCurrentWallet } from './getCurrentWallet.js';

/**
 * Mutation hook for disconnecting from an active wallet connection, if currently connected.
 */
export function getDisconnectWallet(state: StoreState) {
	const { setWalletDisconnected } = state;
	const { currentWallet } = getCurrentWallet(state);

	return async () => {
		if (!currentWallet) {
			throw new WalletNotConnectedError('No wallet is connected.');
		}

		try {
			// Wallets aren't required to implement the disconnect feature, so we'll
			// optionally call the disconnect feature if it exists and reset the UI
			// state on the frontend at a minimum.
			await currentWallet.features['standard:disconnect']?.disconnect();
		} catch (error) {
			console.error('Failed to disconnect the application from the current wallet.', error);
		}

		setWalletDisconnected();
	};
}
