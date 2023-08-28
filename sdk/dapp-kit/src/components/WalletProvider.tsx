// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';
import { createContext, useMemo } from 'react';
import { localStorageAdapter } from '../storageAdapters.js';
import type { StorageAdapter } from '../storageAdapters.js';
import type {
	AdditionallyRequiredWalletFeatureName,
	WalletWithSuiFeatures,
} from '@mysten/wallet-standard';

export interface WalletProviderProps {
	/** */
	preferredWallets?: string[];

	/** */
	storageAdapter?: StorageAdapter;

	/** */
	storageKey?: string;

	/** A list of Wallet Standard features that are required for the dApp to function. This filters the list of wallets presented to users when selecting a wallet to connect from, ensuring that only wallets that meet the dApps requirements can connect. */
	additionalFeatures?: AdditionallyRequiredWalletFeatureName[];

	/** Enables automatically reconnecting to the most recently used wallet upon mounting. */
	autoConnect?: boolean;

	/** Enables the development-only unsafe burner wallet, which can be useful for testing. */
	enableUnsafeBurner?: boolean;

	children: ReactNode;
}

type WalletProviderContext = {
	wallets: WalletWithSuiFeatures[];
	currentWallet: WalletWithSuiFeatures | null;
	accounts: readonly WalletAccount[];
	currentAccount: WalletAccount | null;
	status: WalletKitCoreConnectionStatus;
};

export const DEFAULT_FEATURES: (keyof WalletWithSuiFeatures['features'])[] = [
	'standard:connect',
	'sui:signAndExecuteTransactionBlock',
];

export const WalletContext = createContext<WalletProviderContext | null>(null);

const SUI_WALLET_NAME = 'Sui Wallet';

const RECENT_WALLET_STORAGE = 'wallet-kit:last-wallet';

export function WalletProvider({
	preferredWallets = [SUI_WALLET_NAME],
	features = DEFAULT_FEATURES,
	storageAdapter = localStorageAdapter,
	autoConnect = false,
	storageKey,
	disableAutoConnect,
	enableUnsafeBurner,
	children,
}: WalletProviderProps) {
	const ctx = useMemo((): WalletProviderContext => {
		return {};
	}, []);

	return <WalletContext.Provider value={ctx}>{children}</WalletContext.Provider>;
}
