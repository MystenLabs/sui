// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet, WalletAccount, WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { persistentAtom } from '@nanostores/persistent';
import { map } from 'nanostores';

import {
	DEFAULT_PREFERRED_WALLETS,
	DEFAULT_STORAGE_KEY_V2,
	DEFAULT_WALLET_FILTER,
} from '../../constants/walletDefaults.js';
import { getRegisteredWallets, getWalletUniqueIdentifier } from '../../utils/walletUtils.js';

export type DappKitStateOptions = {
	autoConnectEnabled?: boolean;
	preferredWallets?: string[];
	walletFilter?: (wallet: WalletWithRequiredFeatures) => boolean;
	storageKey?: string;
};

export type WalletConnectionStatus = 'disconnected' | 'connecting' | 'connected';
export type AutoConnectStatus = 'disabled' | 'idle' | 'attempted';

export type DappKitStoreState = {
	autoConnectEnabled: boolean;
	autoConnectStatus: AutoConnectStatus;
	wallets: WalletWithRequiredFeatures[];
	accounts: readonly WalletAccount[];
	currentWallet: WalletWithRequiredFeatures | null;
	currentAccount: WalletAccount | null;
	connectionStatus: WalletConnectionStatus;
	supportedIntents: string[];
};

export type LastConnectionStoreState = {
	walletName?: string | null;
	accountAddress?: string | null;
};

export function createState({
	autoConnectEnabled = true,
	preferredWallets = DEFAULT_PREFERRED_WALLETS,
	walletFilter = DEFAULT_WALLET_FILTER,
	storageKey = DEFAULT_STORAGE_KEY_V2,
}: DappKitStateOptions) {
	const $state = map<DappKitStoreState>({
		autoConnectEnabled,
		autoConnectStatus: autoConnectEnabled ? 'idle' : 'disabled',
		wallets: getRegisteredWallets(preferredWallets, walletFilter),
		accounts: [] as WalletAccount[],
		currentWallet: null,
		currentAccount: null,
		connectionStatus: 'disconnected',
		supportedIntents: [],
	});

	const $recentConnection = persistentAtom<LastConnectionStoreState>(
		storageKey,
		{
			walletName: null,
			accountAddress: null,
		},
		{
			encode: JSON.stringify,
			decode: JSON.parse,
		},
	);

	const actions = {
		setConnectionStatus(connectionStatus: WalletConnectionStatus) {
			$state.setKey('connectionStatus', connectionStatus);
		},
		setWalletConnected(
			wallet: WalletWithRequiredFeatures,
			connectedAccounts: readonly WalletAccount[],
			selectedAccount: WalletAccount | null,
			supportedIntents: string[] = [],
		) {
			$state.set({
				...$state.get(),
				accounts: connectedAccounts,
				currentWallet: wallet,
				currentAccount: selectedAccount,
				connectionStatus: 'connected',
				supportedIntents,
			});

			$recentConnection.set({
				walletName: getWalletUniqueIdentifier(wallet),
				accountAddress: selectedAccount?.address,
			});
		},
		setWalletDisconnected() {
			$state.set({
				...$state.get(),
				accounts: [],
				currentWallet: null,
				currentAccount: null,
				connectionStatus: 'disconnected',
				supportedIntents: [],
			});

			$recentConnection.set({
				walletName: null,
				accountAddress: null,
			});
		},
		setAccountSwitched(selectedAccount: WalletAccount) {
			$state.setKey('currentAccount', selectedAccount);

			$recentConnection.set({
				...$recentConnection.get(),
				accountAddress: selectedAccount.address,
			});
		},
		setWalletRegistered(updatedWallets: WalletWithRequiredFeatures[]) {
			$state.setKey('wallets', updatedWallets);
		},
		setWalletUnregistered(
			updatedWallets: WalletWithRequiredFeatures[],
			unregisteredWallet: Wallet,
		) {
			if (unregisteredWallet === $state.get().currentWallet) {
				$state.set({
					...$state.get(),
					wallets: updatedWallets,
					accounts: [],
					currentWallet: null,
					currentAccount: null,
					connectionStatus: 'disconnected',
					supportedIntents: [],
				});

				$recentConnection.set({
					walletName: null,
					accountAddress: null,
				});
			} else {
				$state.setKey('wallets', updatedWallets);
			}
		},
		updateWalletAccounts(accounts: readonly WalletAccount[]) {
			const { currentAccount, ...state } = $state.get();

			$state.set({
				...state,
				accounts,
				currentAccount:
					(currentAccount && accounts.find(({ address }) => address === currentAccount.address)) ||
					accounts[0],
			});
		},
	};

	return {
		$state,
		$recentConnection,
		actions,
	};
}
