// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useDAppKitStore } from '../useDAppKitStore.js';
import { useConnectWallet } from './useConnectWallet.js';

export function useAutoConnectWallet(autoConnectEnabled: boolean) {
	const { mutate: connectWallet } = useConnectWallet();
	const wallets = useDAppKitStore((state) => state.wallets);
	const lastWalletName = useDAppKitStore((state) => state.lastWalletName);
	const lastAccountAddress = useDAppKitStore((state) => state.lastAccountAddress);

	useEffect(() => {
		if (!autoConnectEnabled) return;

		const wallet = lastWalletName ? wallets.find((wallet) => wallet.name === lastWalletName) : null;
		if (wallet) {
			connectWallet({ wallet, accountAddress: lastAccountAddress || undefined, silent: true });
		}
	}, [autoConnectEnabled, connectWallet, lastAccountAddress, lastWalletName, wallets]);
}
