// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet, WalletAccount, WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { createStore } from 'zustand';
import type { StateStorage } from 'zustand/middleware';
import { createJSONStorage, persist } from 'zustand/middleware';

export type WalletActions = {
	setAccountSwitched: (selectedAccount: WalletAccount) => void;
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
} & WalletActions;

export type WalletConfiguration = {
	wallets: WalletWithRequiredFeatures[];
	storage: StateStorage;
	storageKey: string;
};

export function createWalletStore({ wallets, storage, storageKey }: WalletConfiguration) {
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
				setWalletConnected(wallet, connectedAccounts, selectedAccount) {
					set(() => ({
						accounts: connectedAccounts,
						currentWallet: wallet,
						currentAccount: selectedAccount,
						lastConnectedWalletName: wallet.name,
						lastConnectedAccountAddress: selectedAccount?.address,
					}));
				},
				setWalletDisconnected() {
					set(() => ({
						accounts: [],
						currentWallet: null,
						currentAccount: null,
						lastConnectedWalletName: null,
						lastConnectedAccountAddress: null,
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
				storage: createJSONStorage(() => storage),
				partialize: ({ lastConnectedWalletName, lastConnectedAccountAddress }) => ({
					lastConnectedWalletName,
					lastConnectedAccountAddress,
				}),
			},
		),
	);
}
