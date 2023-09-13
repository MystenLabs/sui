// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { useUnsafeBurnerWallet } from '../hooks/wallet/useUnsafeBurnerWallet.js';
import { useWalletsChanged } from '../hooks/wallet/useWalletsChanged.js';
import type { ReactNode } from 'react';
import { useAutoConnectWallet } from '../hooks/wallet/useAutoConnectWallet.js';
// import { useEffect } from 'react';
// import { useConnectWallet } from '../hooks/wallet/useConnectWallet.js';
// import { useDAppKitStore, useDAppKitStore } from '../hooks/useDAppKitStore.js';

type WalletConnectionManagerProps = {
	/** A list of wallets that are sorted to the top of the wallet list, if they are available to connect to. By default, wallets are sorted by the order they are loaded in. */
	preferredWallets: string[];

	/** A list of features that are required for the dApp to function. This filters the list of wallets presented to users when selecting a wallet to connect from, ensuring that only wallets that meet the dApps requirements can connect. */
	requiredFeatures: (keyof WalletWithRequiredFeatures['features'])[];

	/** Enables the development-only unsafe burner wallet, which can be useful for testing. */
	enableUnsafeBurner: boolean;

	/** Enables automatically reconnecting to the most recently used wallet account upon mounting. */
	autoConnect: boolean;

	children: ReactNode;
};

export function WalletConnectionManager({
	preferredWallets,
	requiredFeatures,
	enableUnsafeBurner,
	autoConnect,
	children,
}: WalletConnectionManagerProps) {
	useWalletsChanged(preferredWallets, requiredFeatures);
	useUnsafeBurnerWallet(enableUnsafeBurner);
	useAutoConnectWallet(autoConnect);

	return children;
}
