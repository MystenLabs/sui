// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';
import { useRef } from 'react';
import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { createDAppKitStore } from '../store.js';
import type { StateStorage } from 'zustand/middleware';
import { DAppKitContext } from '../contexts/dAppKitContext.js';
import { getRegisteredWallets } from '../utils/walletUtils.js';
import { WalletConnectionManager } from './WalletConnectionManager.js';

type DAppKitProviderProps = {
	/** A list of wallets that are sorted to the top of the wallet list, if they are available to connect to. By default, wallets are sorted by the order they are loaded in. */
	preferredWallets?: string[];

	/** A list of features that are required for the dApp to function. This filters the list of wallets presented to users when selecting a wallet to connect from, ensuring that only wallets that meet the dApps requirements can connect. */
	requiredFeatures?: (keyof WalletWithRequiredFeatures['features'])[];

	/** Enables the development-only unsafe burner wallet, which can be useful for testing. */
	enableUnsafeBurner?: boolean;

	/** Enables automatically reconnecting to the most recently used wallet account upon mounting. */
	autoConnect?: boolean;

	/** Configures how the most recently connected to wallet account is stored. Defaults to using localStorage. */
	storage?: StateStorage;

	/** The key to use to store the most recently connected wallet account. */
	storageKey?: string;

	children: ReactNode;
};

const SUI_WALLET_NAME = 'Sui Wallet';
const DEFUALT_STORAGE_KEY = 'sui-dapp-kit:wallet-connection-info';

export function DAppKitProvider({
	preferredWallets = [SUI_WALLET_NAME],
	requiredFeatures = [],
	storage = localStorage,
	storageKey = DEFUALT_STORAGE_KEY,
	enableUnsafeBurner = false,
	autoConnect = false,
	children,
}: DAppKitProviderProps) {
	const storeRef = useRef(
		createDAppKitStore({
			wallets: getRegisteredWallets(preferredWallets, requiredFeatures),
			storageKey,
			storage,
		}),
	);

	return (
		<DAppKitContext.Provider value={storeRef.current}>
			<WalletConnectionManager
				preferredWallets={preferredWallets}
				requiredFeatures={requiredFeatures}
				enableUnsafeBurner={enableUnsafeBurner}
				autoConnect={autoConnect}
			>
				{children}
			</WalletConnectionManager>
		</DAppKitContext.Provider>
	);
}
