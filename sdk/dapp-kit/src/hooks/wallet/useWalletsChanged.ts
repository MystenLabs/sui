// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { getWallets } from '@mysten/wallet-standard';
import { useEffect } from 'react';

import { getRegisteredWallets } from '../../utils/walletUtils.js';
import { useWalletStore } from './useWalletStore.js';

/**
 * Internal hook for easily handling the addition and removal of new wallets.
 */
export function useWalletsChanged(
	preferredWallets: string[],
	requiredFeatures: (keyof WalletWithRequiredFeatures['features'])[],
) {
	const setWalletRegistered = useWalletStore((state) => state.setWalletRegistered);
	const setWalletUnregistered = useWalletStore((state) => state.setWalletUnregistered);

	useEffect(() => {
		const walletsApi = getWallets();

		const unsubscribeFromRegister = walletsApi.on('register', () => {
			setWalletRegistered(getRegisteredWallets(preferredWallets, requiredFeatures));
		});

		const unsubscribeFromUnregister = walletsApi.on('unregister', (unregisteredWallet) => {
			setWalletUnregistered(
				getRegisteredWallets(preferredWallets, requiredFeatures),
				unregisteredWallet,
			);
		});

		return () => {
			unsubscribeFromRegister();
			unsubscribeFromUnregister();
		};
	}, [preferredWallets, requiredFeatures, setWalletRegistered, setWalletUnregistered]);
}
