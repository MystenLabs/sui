// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useDAppKitStore } from '../useDAppKitStore.js';
import { useConnectWallet } from './useConnectWallet.js';

export function useAutoConnectWallet(autoConnectEnabled: boolean) {
	const { mutate: connectWallet } = useConnectWallet();
	const wallets = useDAppKitStore((state) => state.wallets);
	const lastConnectedWalletName = useDAppKitStore((state) => state.lastConnectedWalletName);
	const lastConnectedAccountAddress = useDAppKitStore((state) => state.lastConnectedAccountAddress);

	useEffect(() => {
		if (!autoConnectEnabled) return;

		const wallet = lastConnectedWalletName
			? wallets.find((wallet) => wallet.name === lastConnectedWalletName)
			: null;
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
