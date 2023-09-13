// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet, WalletAccount, WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import type { StateCreator } from 'zustand';

export type WalletSlice = {
	wallets: WalletWithRequiredFeatures[];
	lastWalletName: string | null;
	lastAccountAddress: string | null;
	currentWallet: WalletWithRequiredFeatures | null;
	currentAccount: WalletAccount | null;
	connectionStatus: 'disconnected' | 'connected';
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
				connectionStatus: 'connected',
			}));
		},
		setWalletDisconnected: () => {
			set(() => ({
				currentWallet: null,
				currentAccount: null,
				connectionStatus: 'disconnected',
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
					connectionStatus: 'disconnected',
				}));
			} else {
				set(() => ({ wallets: updatedWallets }));
			}
		},
	});
}
