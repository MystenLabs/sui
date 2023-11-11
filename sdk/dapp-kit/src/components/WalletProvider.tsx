// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import type { ReactNode } from 'react';
import { useRef } from 'react';
import type { StateStorage } from 'zustand/middleware';

import { WalletContext } from '../contexts/walletContext.js';
import { useAutoConnectWallet } from '../hooks/wallet/useAutoConnectWallet.js';
import { useUnsafeBurnerWallet } from '../hooks/wallet/useUnsafeBurnerWallet.js';
import { useWalletPropertiesChanged } from '../hooks/wallet/useWalletPropertiesChanged.js';
import { useWalletsChanged } from '../hooks/wallet/useWalletsChanged.js';
import { lightTheme } from '../themes/lightTheme.js';
import type { Theme } from '../themes/themeContract.js';
import { getRegisteredWallets } from '../utils/walletUtils.js';
import { createWalletStore } from '../walletStore.js';
import { InjectedThemeStyles } from './styling/InjectedThemeStyles.js';

type WalletProviderProps = {
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

	/** The theme to use for styling UI components. Defaults to using the light theme. */
	theme?: Theme | null;

	children: ReactNode;
};

const SUI_WALLET_NAME = 'Sui Wallet';

const DEFAULT_STORAGE_KEY = 'sui-dapp-kit:wallet-connection-info';

const DEFAULT_REQUIRED_FEATURES: (keyof WalletWithRequiredFeatures['features'])[] = [
	'sui:signTransactionBlock',
];

export function WalletProvider({
	preferredWallets = [SUI_WALLET_NAME],
	requiredFeatures = DEFAULT_REQUIRED_FEATURES,
	storage = localStorage,
	storageKey = DEFAULT_STORAGE_KEY,
	enableUnsafeBurner = false,
	autoConnect = false,
	theme = lightTheme,
	children,
}: WalletProviderProps) {
	const storeRef = useRef(
		createWalletStore({
			wallets: getRegisteredWallets(preferredWallets, requiredFeatures),
			storageKey,
			storage,
		}),
	);

	return (
		<WalletContext.Provider value={storeRef.current}>
			<WalletConnectionManager
				preferredWallets={preferredWallets}
				requiredFeatures={requiredFeatures}
				enableUnsafeBurner={enableUnsafeBurner}
				autoConnect={autoConnect}
			>
				{/* TODO: We ideally don't want to inject styles if people aren't using the UI components */}
				{theme ? <InjectedThemeStyles theme={theme} /> : null}
				{children}
			</WalletConnectionManager>
		</WalletContext.Provider>
	);
}

type WalletConnectionManagerProps = Required<
	Pick<
		WalletProviderProps,
		'preferredWallets' | 'requiredFeatures' | 'enableUnsafeBurner' | 'autoConnect' | 'children'
	>
>;

function WalletConnectionManager({
	preferredWallets,
	requiredFeatures,
	enableUnsafeBurner,
	autoConnect,
	children,
}: WalletConnectionManagerProps) {
	useWalletsChanged(preferredWallets, requiredFeatures);
	useWalletPropertiesChanged();
	useUnsafeBurnerWallet(enableUnsafeBurner);
	useAutoConnectWallet(autoConnect);

	return children;
}
