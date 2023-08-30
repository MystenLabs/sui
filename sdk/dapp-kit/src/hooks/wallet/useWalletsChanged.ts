// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet } from '@mysten/wallet-standard';
import { getWallets } from '@mysten/wallet-standard';
import { useEffect } from 'react';

/**
 * Internal hook for easily handling the addition and removal of wallets.
 * @param onWalletsChanged
 */
export function useWalletsChanged(onWalletsChanged: (wallet: Wallet) => void) {
	useEffect(() => {
		const walletsApi = getWallets();
		const unsubscribeFromRegister = walletsApi.on('register', onWalletsChanged);
		const unsubscribeFromUnregister = walletsApi.on('unregister', onWalletsChanged);

		return () => {
			unsubscribeFromRegister();
			unsubscribeFromUnregister();
		};
	}, [onWalletsChanged]);
}
