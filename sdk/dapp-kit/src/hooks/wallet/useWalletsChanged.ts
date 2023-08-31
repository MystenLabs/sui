// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet } from '@mysten/wallet-standard';
import { getWallets } from '@mysten/wallet-standard';
import { useEffect } from 'react';

/**
 * Internal hook for easily handling the addition and removal of new wallets.
 */
export function useWalletsChanged({
	onWalletRegistered,
	onWalletUnregistered,
}: {
	onWalletRegistered: (wallet: Wallet) => void;
	onWalletUnregistered: (wallet: Wallet) => void;
}) {
	useEffect(() => {
		const walletsApi = getWallets();
		const unsubscribeFromRegister = walletsApi.on('register', onWalletRegistered);
		const unsubscribeFromUnregister = walletsApi.on('unregister', onWalletUnregistered);

		return () => {
			unsubscribeFromRegister();
			unsubscribeFromUnregister();
		};
	}, [onWalletRegistered, onWalletUnregistered]);
}
