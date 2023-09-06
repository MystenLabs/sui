// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Dispatch, ReactNode } from 'react';
import { createContext, useContext, useMemo, useReducer } from 'react';
import { getWallets } from '@mysten/wallet-standard';
import { localStorageAdapter } from '../utils/storageAdapters.js';
import type { StorageAdapter } from '../utils/storageAdapters.js';
import { walletReducer } from '../reducers/walletReducer.js';
import type { WalletAction, WalletState } from '../reducers/walletReducer.js';
import { sortWallets } from '../utils/walletUtils.js';

interface WalletProviderProps {
	/** A list of wallets that are sorted to the top of the wallet list, if they are available to connect to. By default, wallets are sorted by the order they are loaded in. */
	preferredWallets?: string[];

	/** Configures how the most recently connected to wallet account is stored. Defaults to using localStorage. */
	storageAdapter?: StorageAdapter;

	/** The key to use to store the most recently connected wallet account. */
	storageKey?: string;

	/** A list of features that are required for the dApp to function. This filters the list of wallets presented to users when selecting a wallet to connect from, ensuring that only wallets that meet the dApps requirements can connect. */
	requiredFeatures?: string[];

	/** Enables automatically reconnecting to the most recently used wallet account upon mounting. */
	autoConnect?: boolean;

	/** Enables the development-only unsafe burner wallet, which can be useful for testing. */
	enableUnsafeBurner?: boolean;

	children: ReactNode;
}

interface WalletProviderContext extends WalletState {
	dispatch: Dispatch<WalletAction>;
	storageAdapter: StorageAdapter;
	storageKey: string;
}

const SUI_WALLET_NAME = 'Sui Wallet';
const DEFAULT_STORAGE_KEY = 'dapp-kit:most-recent-wallet-connection-info';

const WalletContext = createContext<WalletProviderContext | null>(null);

export function WalletProvider({
	preferredWallets = [SUI_WALLET_NAME],
	requiredFeatures = [],
	storageAdapter = localStorageAdapter,
	storageKey = DEFAULT_STORAGE_KEY,
	children,
}: WalletProviderProps) {
	const walletsApi = getWallets();
	const registeredWallets = walletsApi.get();
	const [walletState, dispatch] = useReducer(walletReducer, {
		wallets: sortWallets(registeredWallets, preferredWallets, requiredFeatures),
		currentWallet: null,
		accounts: [],
		currentAccount: null,
		connectionStatus: 'disconnected',
	});

	// Memo-ize the context value so we don't trigger un-necessary re-renders from
	// ancestor components higher in the component tree.
	const contextValue = useMemo(
		() => ({ ...walletState, storageAdapter, storageKey, dispatch }),
		[storageAdapter, storageKey, walletState],
	);
	return <WalletContext.Provider value={contextValue}>{children}</WalletContext.Provider>;
}

export function useWalletContext() {
	const context = useContext(WalletContext);
	if (!context) {
		throw new Error(
			'Could not find WalletContext. Ensure that you have set up the WalletProvider.',
		);
	}
	return context;
}
