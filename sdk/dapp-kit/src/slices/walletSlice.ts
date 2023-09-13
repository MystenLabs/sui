// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet, WalletAccount, WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import type { StateCreator } from 'zustand';

export type WalletActions = {
	setAccountSwitched: (selectedAccount: WalletAccount) => void;
	setWalletConnected: (
		wallet: WalletWithRequiredFeatures,
		selectedAccount: WalletAccount | null,
	) => void;
	setWalletDisconnected: () => void;
	setWalletRegistered: (updatedWallets: WalletWithRequiredFeatures[]) => void;
	setWalletUnregistered: (
		updatedWallets: WalletWithRequiredFeatures[],
		unregisteredWallet: Wallet,
	) => void;
};

export type WalletSlice = {
	wallets: WalletWithRequiredFeatures[];
	currentAccount: WalletAccount | null;
	lastConnectedAccountAddress: string | null;
} & WalletActions &
	(
		| {
				lastConnectedWalletName: null;
				currentWallet: null;
				connectionStatus: 'disconnected';
		  }
		| {
				lastConnectedWalletName: string;
				currentWallet: WalletWithRequiredFeatures;
				connectionStatus: 'connected';
		  }
	);

export function createWalletSlice(
	initialWallets: WalletWithRequiredFeatures[],
): StateCreator<
	WalletSlice,
	[],
	[['zustand/subscribeWithSelector', never], ['zustand/persist', unknown]],
	WalletSlice
> {
	return (set, get) => ({
		wallets: initialWallets,
		currentWallet: null,
		currentAccount: null,
		lastConnectedAccountAddress: null,
		lastConnectedWalletName: null,
		connectionStatus: 'disconnected',
		setWalletConnected: (wallet, selectedAccount) => {
			set(() => ({
				currentWallet: wallet,
				currentAccount: selectedAccount,
				lastConnectedWalletName: wallet.name,
				lastConnectedAccountAddress: selectedAccount?.address,
				connectionStatus: 'connected',
			}));
		},
		setWalletDisconnected: () => {
			set(() => ({
				currentWallet: null,
				currentAccount: null,
				lastConnectedWalletName: null,
				lastConnectedAccountAddress: null,
				connectionStatus: 'disconnected',
			}));
		},
		setAccountSwitched: (selectedAccount) => {
			set(() => ({
				currentAccount: selectedAccount,
				lastConnectedAccountAddress: selectedAccount.address,
			}));
		},
		setWalletRegistered: (updatedWallets) => {
			set(() => ({ wallets: updatedWallets }));
		},
		setWalletUnregistered: (updatedWallets, unregisteredWallet) => {
			if (unregisteredWallet === get().currentWallet) {
				set(() => ({
					wallets: updatedWallets,
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
	});
}
