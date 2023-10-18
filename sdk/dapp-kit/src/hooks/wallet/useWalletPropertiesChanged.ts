// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';

import { useCurrentWallet } from './useCurrentWallet.js';
import { useWalletStore } from './useWalletStore.js';

/**
 * Internal hook for easily handling various changes in properties for a wallet.
 */
export function useWalletPropertiesChanged() {
	const { currentWallet } = useCurrentWallet();
	const updateWalletAccounts = useWalletStore((state) => state.updateWalletAccounts);

	useEffect(() => {
		const unsubscribeFromEvents = currentWallet?.features['standard:events'].on(
			'change',
			({ accounts }) => {
				// TODO: We should handle features changing that might make the list of wallets
				// or even the current wallet incompatible with the dApp.
				if (accounts) {
					updateWalletAccounts(accounts);
				}
			},
		);
		return unsubscribeFromEvents;
	}, [currentWallet?.features, updateWalletAccounts]);
}
