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
	lastAccountAddress: string | null;
} & WalletActions &
	(
		| {
				lastWalletName: null;
				currentWallet: null;
				connectionStatus: 'disconnected';
		  }
		| {
				lastWalletName: string;
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
		lastAccountAddress: null,
		lastWalletName: null,
		connectionStatus: 'disconnected',
		setWalletConnected: (wallet, selectedAccount) => {
			set(() => ({
				currentWallet: wallet,
				currentAccount: selectedAccount,
				lastWalletName: wallet.name,
				lastAccountAddress: selectedAccount?.address,
				connectionStatus: 'connected',
			}));
		},
		setWalletDisconnected: () => {
			set(() => ({
				currentWallet: null,
				currentAccount: null,
				lastWalletName: null,
				lastAccountAddress: null,
				connectionStatus: 'disconnected',
			}));
		},
		setAccountSwitched: (selectedAccount) => {
			set(() => ({
				currentAccount: selectedAccount,
				lastAccountAddress: selectedAccount.address,
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
					lastWalletName: null,
					lastAccountAddress: null,
					connectionStatus: 'disconnected',
				}));
			} else {
				set(() => ({ wallets: updatedWallets }));
			}
		},
	});
}
