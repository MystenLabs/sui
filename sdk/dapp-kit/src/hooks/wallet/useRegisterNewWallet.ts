// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet } from '@mysten/wallet-standard';
import { getWallets } from '@mysten/wallet-standard';
import { useEffect } from 'react';

export function useRegisterNewWallet(onRegister: (wallets: Wallet[]) => void) {
	useEffect(() => {
		const walletsApi = getWallets();
		const unsubscribeFromRegister = walletsApi.on('register', onRegister);
		const unsubscribeFromUnregister = walletsApi.on('unregister', onRegister);

		return () => {
			unsubscribeFromRegister();
			unsubscribeFromUnregister();
		};
	}, [onRegister]);
}
