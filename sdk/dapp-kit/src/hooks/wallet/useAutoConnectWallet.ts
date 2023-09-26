// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';

import { useConnectWallet } from './useConnectWallet.js';
import { useWallets } from './useWallets.js';
import { useWalletStore } from './useWalletStore.js';

export function useAutoConnectWallet(autoConnectEnabled: boolean) {
	const { mutate: connectWallet } = useConnectWallet();
	const wallets = useWallets();
	const lastConnectedWalletName = useWalletStore((state) => state.lastConnectedWalletName);
	const lastConnectedAccountAddress = useWalletStore((state) => state.lastConnectedAccountAddress);

	useEffect(() => {
		if (!autoConnectEnabled || !lastConnectedWalletName) return;

		const wallet = wallets.find((wallet) => wallet.name === lastConnectedWalletName);
		if (wallet) {
			connectWallet({
				wallet,
				accountAddress: lastConnectedAccountAddress || undefined,
				silent: true,
			});
		}
	}, [
		autoConnectEnabled,
		connectWallet,
		lastConnectedAccountAddress,
		lastConnectedWalletName,
		wallets,
	]);
}
