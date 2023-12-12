// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet, WalletAccount, WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { createStore } from 'zustand';
import type { StateStorage } from 'zustand/middleware';
import { createJSONStorage, persist } from 'zustand/middleware';

import { getWalletUniqueIdentifier } from './utils/walletUtils.js';

type WalletConnectionStatus = 'disconnected' | 'connecting' | 'connected';

export type WalletActions = {
	setAccountSwitched: (selectedAccount: WalletAccount) => void;
	setConnectionStatus: (connectionStatus: WalletConnectionStatus) => void;
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
	autoConnectEnabled: boolean;
	wallets: WalletWithRequiredFeatures[];
	accounts: readonly WalletAccount[];
	currentWallet: WalletWithRequiredFeatures | null;
	currentAccount: WalletAccount | null;
	lastConnectedAccountAddress: string | null;
	lastConnectedWalletName: string | null;
	connectionStatus: WalletConnectionStatus;
} & WalletActions;

type WalletConfiguration = {
	autoConnectEnabled: boolean;
	wallets: WalletWithRequiredFeatures[];
	storage: StateStorage;
	storageKey: string;
};

export function createWalletStore({
	wallets,
	storage,
	storageKey,
	autoConnectEnabled,
}: WalletConfiguration) {
	return createStore<StoreState>()(
		persist(
			(set, get) => ({
				autoConnectEnabled,
				wallets,
				accounts: [] as WalletAccount[],
				currentWallet: null,
				currentAccount: null,
				lastConnectedAccountAddress: null,
				lastConnectedWalletName: null,
				connectionStatus: 'disconnected',
				setConnectionStatus(connectionStatus) {
					set(() => ({
						connectionStatus,
					}));
				},
				setWalletConnected(wallet, connectedAccounts, selectedAccount) {
					set(() => ({
						accounts: connectedAccounts,
						currentWallet: wallet,
						currentAccount: selectedAccount,
						lastConnectedWalletName: getWalletUniqueIdentifier(wallet),
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
						currentAccount:
							(currentAccount &&
								accounts.find(({ address }) => address === currentAccount.address)) ||
							accounts[0],
					}));
				},
			}),
			{
				name: storageKey,
				storage: createJSONStorage(() => storage),
				partialize: ({ lastConnectedWalletName, lastConnectedAccountAddress }) => ({
					lastConnectedWalletName,
					lastConnectedAccountAddress,
				}),
			},
		),
	);
}
