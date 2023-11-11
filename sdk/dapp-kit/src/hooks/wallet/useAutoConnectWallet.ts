// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useLayoutEffect } from 'react';

import { useConnectWallet } from './useConnectWallet.js';
import { useCurrentWallet } from './useCurrentWallet.js';
import { useWallets } from './useWallets.js';
import { useWalletStore } from './useWalletStore.js';

export function useAutoConnectWallet(autoConnectEnabled: boolean) {
	const { mutate: connectWallet } = useConnectWallet();
	const lastConnectedWalletName = useWalletStore((state) => state.lastConnectedWalletName);
	const lastConnectedAccountAddress = useWalletStore((state) => state.lastConnectedAccountAddress);
	const wallets = useWallets();
	const { isDisconnected } = useCurrentWallet();

	useLayoutEffect(() => {
		if (
			!autoConnectEnabled ||
			!lastConnectedWalletName ||
			!lastConnectedAccountAddress ||
			!isDisconnected
		) {
			return;
		}

		const wallet = wallets.find((wallet) => wallet.name === lastConnectedWalletName);
		if (wallet) {
			connectWallet({
				wallet,
				accountAddress: lastConnectedAccountAddress,
				silent: true,
			});
		}
	}, [
		autoConnectEnabled,
		connectWallet,
		isDisconnected,
		lastConnectedAccountAddress,
		lastConnectedWalletName,
		wallets,
	]);
}
