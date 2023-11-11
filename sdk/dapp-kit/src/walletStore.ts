// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet, WalletAccount, WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { createStore } from 'zustand';
import type { StateStorage } from 'zustand/middleware';
import { createJSONStorage, persist } from 'zustand/middleware';

import { DEFAULT_STORAGE_KEY } from './constants/defaults.js';

type WalletConnectionStatus = 'disconnected' | 'connecting' | 'connected';

type WalletAutoConnectionStatus = 'disabled' | 'idle' | 'settled';

export type WalletActions = {
	setAccountSwitched: (selectedAccount: WalletAccount) => void;
	setConnectionStatus: (connectionStatus: WalletConnectionStatus) => void;
	setAutoConnectionStatus: (autoConnectionStatus: WalletAutoConnectionStatus) => void;
	setWalletConnected: (
		wallet: WalletWithRequiredFeatures,
		connectedAccounts: readonly WalletAccount[],
		selectedAccount: WalletAccount | null,
	) => void;
	updateWalletAccounts: (accounts: readonly WalletAccount[]) => void;
	setWalletDisconnected: () => void;
	setWalletRegistered: (updatedWallets: WalletWithRequiredFeatures[]) => void;
	setWalletUnregistered: (
		updatedWallets: WalletWithRequiredFeatures[],
		unregisteredWallet: Wallet,
	) => void;
};

export type WalletStore = ReturnType<typeof createWalletStore>;

export type StoreState = {
	wallets: WalletWithRequiredFeatures[];
	accounts: readonly WalletAccount[];
	currentWallet: WalletWithRequiredFeatures | null;
	currentAccount: WalletAccount | null;
	lastConnectedAccountAddress: string | null;
	lastConnectedWalletName: string | null;
	connectionStatus: WalletConnectionStatus;
	autoConnectionStatus: WalletAutoConnectionStatus;
} & WalletActions;

export type PersistedState = {
	lastConnectedWalletName: string;
	lastConnectedAccountAddress: string;
};

type WalletConfiguration = {
	wallets: WalletWithRequiredFeatures[];
	autoConnect: boolean;
	storage?: StateStorage | null;
	storageKey: string;
};

export async function readStorageState(
	storage: StateStorage,
	storageKey: string = DEFAULT_STORAGE_KEY,
): Promise<PersistedState | null> {
	const store = createJSONStorage<PersistedState>(() => storage);
	if (!store) return null;

	const persisted = await store.getItem(storageKey);
	if (!persisted) return null;

	return persisted.state;
}

function createInMemoryStore(): StateStorage {
	const store = new Map();
	return {
		getItem(key: string) {
			return store.get(key);
		},
		setItem(key: string, value: string) {
			store.set(key, value);
		},
		removeItem(key: string) {
			store.delete(key);
		},
	};
}

export function createWalletStore({
	wallets,
	storage,
	storageKey,
	autoConnect,
}: WalletConfiguration) {
	return createStore<StoreState>()(
		persist(
			(set, get) => ({
				wallets,
				accounts: [],
				currentWallet: null,
				currentAccount: null,
				lastConnectedAccountAddress: null,
				lastConnectedWalletName: null,
				connectionStatus: 'disconnected',
				autoConnectionStatus: autoConnect ? 'idle' : 'disabled',
				setConnectionStatus(connectionStatus) {
					set(() => ({
						connectionStatus,
					}));
				},
				setAutoConnectionStatus(autoConnectionStatus) {
					set(() => ({
						autoConnectionStatus,
					}));
				},
				setWalletConnected(wallet, connectedAccounts, selectedAccount) {
					set(() => ({
						accounts: connectedAccounts,
						currentWallet: wallet,
						currentAccount: selectedAccount,
						lastConnectedWalletName: wallet.name,
						lastConnectedAccountAddress: selectedAccount?.address,
						connectionStatus: 'connected',
					}));
				},
				setWalletDisconnected() {
					set(() => ({
						accounts: [],
						currentWallet: null,
						currentAccount: null,
						lastConnectedWalletName: null,
						lastConnectedAccountAddress: null,
						connectionStatus: 'disconnected',
					}));
				},
				setAccountSwitched(selectedAccount) {
					set(() => ({
						currentAccount: selectedAccount,
						lastConnectedAccountAddress: selectedAccount.address,
					}));
				},
				setWalletRegistered(updatedWallets) {
					set(() => ({ wallets: updatedWallets }));
				},
				setWalletUnregistered(updatedWallets, unregisteredWallet) {
					if (unregisteredWallet === get().currentWallet) {
						set(() => ({
							wallets: updatedWallets,
							accounts: [],
							currentWallet: null,
							currentAccount: null,
							lastConnectedWalletName: null,
							lastConnectedAccountAddress: null,
							connectionStatus: 'disconnected',
						}));
					} else {
						set(() => ({ wallets: updatedWallets }));
					}
				},
				updateWalletAccounts(accounts) {
					const currentAccount = get().currentAccount;

					set(() => ({
						accounts,
						currentAccount: currentAccount
							? accounts.find(({ address }) => address === currentAccount.address)
							: accounts[0],
					}));
				},
			}),
			{
				name: storageKey,
				storage: createJSONStorage(() =>
					storage === null || typeof window === 'undefined'
						? createInMemoryStore()
						: storage || localStorage,
				),
				partialize: ({ lastConnectedWalletName, lastConnectedAccountAddress }) => ({
					lastConnectedWalletName,
					lastConnectedAccountAddress,
				}),
			},
		),
	);
}
