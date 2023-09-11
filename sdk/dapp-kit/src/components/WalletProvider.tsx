// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';
import { useCallback, useMemo, useReducer } from 'react';
import type { Wallet, WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { getWallets } from '@mysten/wallet-standard';
import { localStorageAdapter } from '../utils/storageAdapters.js';
import type { StorageAdapter } from '../utils/storageAdapters.js';
import { walletReducer } from '../reducers/walletReducer.js';
import { useUnsafeBurnerWallet } from '../hooks/wallet/useUnsafeBurnerWallet.js';
import { useWalletsChanged } from '../hooks/wallet/useWalletsChanged.js';
import { WalletContext } from '../contexts/WalletContext.js';
import { sortWallets } from '../utils/walletUtils.js';
import { AutoConnectWallet } from './AutoConnectWallet.js';
interface WalletProviderProps {
	/** A list of wallets that are sorted to the top of the wallet list, if they are available to connect to. By default, wallets are sorted by the order they are loaded in. */
	preferredWallets?: string[];

	/** Configures how the most recently connected to wallet account is stored. Defaults to using localStorage. */
	storageAdapter?: StorageAdapter;

	/** The key to use to store the most recently connected wallet account. */
	storageKey?: string;

	/** A list of features that are required for the dApp to function. This filters the list of wallets presented to users when selecting a wallet to connect from, ensuring that only wallets that meet the dApps requirements can connect. */
	requiredFeatures?: (keyof WalletWithRequiredFeatures['features'])[];

	/** Enables automatically reconnecting to the most recently used wallet account upon mounting. */
	autoConnect?: boolean;

	/** Enables the development-only unsafe burner wallet, which can be useful for testing. */
	enableUnsafeBurner?: boolean;

	children: ReactNode;
}

const SUI_WALLET_NAME = 'Sui Wallet';
const DEFAULT_STORAGE_KEY = 'sui-dapp-kit:wallet-connection-info';

export function WalletProvider({
	preferredWallets = [SUI_WALLET_NAME],
	requiredFeatures = [],
	storageAdapter = localStorageAdapter,
	storageKey = DEFAULT_STORAGE_KEY,
	enableUnsafeBurner = false,
	autoConnect = false,
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

	const onWalletRegistered = useCallback(() => {
		dispatch({
			type: 'wallet-registered',
			payload: {
				updatedWallets: sortWallets(walletsApi.get(), preferredWallets, requiredFeatures),
			},
		});
	}, [preferredWallets, requiredFeatures, walletsApi]);

	const onWalletUnregistered = useCallback(
		(unregisteredWallet: Wallet) => {
			dispatch({
				type: 'wallet-unregistered',
				payload: {
					updatedWallets: sortWallets(walletsApi.get(), preferredWallets, requiredFeatures),
					unregisteredWallet,
				},
			});
		},
		[preferredWallets, requiredFeatures, walletsApi],
	);

	useWalletsChanged({
		onWalletRegistered,
		onWalletUnregistered,
	});

	useUnsafeBurnerWallet(enableUnsafeBurner);

	// Memo-ize the context value so we don't trigger un-necessary re-renders from
	// ancestor components higher in the component tree.
	const contextValue = useMemo(() => ({ ...walletState, dispatch }), [walletState]);
	return (
		<WalletContext.Provider value={contextValue}>
			{autoConnect ? (
				<AutoConnectWallet storageAdapter={storageAdapter} storageKey={storageKey}>
					{children}
				</AutoConnectWallet>
			) : (
				children
			)}
		</WalletContext.Provider>
	);
}
