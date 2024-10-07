// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getWallets } from '@mysten/wallet-standard';
import type { Wallet, WalletAccount, WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { persistentAtom } from '@nanostores/persistent';
import { listenKeys, map, onMount } from 'nanostores';

import {
	DEFAULT_PREFERRED_WALLETS,
	DEFAULT_STORAGE_KEY_V2,
	DEFAULT_WALLET_FILTER,
} from '../constants/walletDefaults.js';
import { getRegisteredWallets, getWalletUniqueIdentifier } from '../utils/walletUtils.js';

type WalletConnectionStatus = 'disconnected' | 'connecting' | 'connected';

type Options = {
	autoConnectEnabled: boolean;
	preferredWallets: string[];
	walletFilter: (wallet: WalletWithRequiredFeatures) => boolean;
	storageKey?: string;
};

type ExperimentalStoreState = {
	autoConnectEnabled: boolean;
	wallets: WalletWithRequiredFeatures[];
	accounts: readonly WalletAccount[];
	currentWallet: WalletWithRequiredFeatures | null;
	currentAccount: WalletAccount | null;
	connectionStatus: WalletConnectionStatus;
	supportedIntents: string[];
};

type LastConnectionStoreState = {
	walletName?: string | null;
	accountAddress?: string | null;
};

export type WalletActions = {
	setAccountSwitched: (selectedAccount: WalletAccount) => void;
	setConnectionStatus: (connectionStatus: WalletConnectionStatus) => void;
	setWalletConnected: (
		wallet: WalletWithRequiredFeatures,
		connectedAccounts: readonly WalletAccount[],
		selectedAccount: WalletAccount | null,
		supportedIntents?: string[],
	) => void;
	updateWalletAccounts: (accounts: readonly WalletAccount[]) => void;
	setWalletDisconnected: () => void;
	setWalletRegistered: (updatedWallets: WalletWithRequiredFeatures[]) => void;
	setWalletUnregistered: (
		updatedWallets: WalletWithRequiredFeatures[],
		unregisteredWallet: Wallet,
	) => void;
};

export function EXPERIMENTAL__createNanoStore({
	autoConnectEnabled,
	preferredWallets = DEFAULT_PREFERRED_WALLETS,
	walletFilter = DEFAULT_WALLET_FILTER,
	storageKey = DEFAULT_STORAGE_KEY_V2,
}: Options) {
	const $store = map<ExperimentalStoreState>({
		autoConnectEnabled,
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

	const $actions: WalletActions = {
		setConnectionStatus(connectionStatus) {
			$store.setKey('connectionStatus', connectionStatus);
		},
		setWalletConnected(wallet, connectedAccounts, selectedAccount, supportedIntents = []) {
			$store.set({
				...$store.get(),
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
			$store.set({
				...$store.get(),
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
		setAccountSwitched(selectedAccount) {
			$store.setKey('currentAccount', selectedAccount);

			$recentConnection.set({
				...$recentConnection.get(),
				accountAddress: selectedAccount.address,
			});
		},
		setWalletRegistered(updatedWallets) {
			$store.setKey('wallets', updatedWallets);
		},
		setWalletUnregistered(updatedWallets, unregisteredWallet) {
			if (unregisteredWallet === $store.get().currentWallet) {
				$store.set({
					...$store.get(),
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
				$store.setKey('wallets', updatedWallets);
			}
		},
		updateWalletAccounts(accounts) {
			const { currentAccount, ...state } = $store.get();

			$store.set({
				...state,
				accounts,
				currentAccount:
					(currentAccount && accounts.find(({ address }) => address === currentAccount.address)) ||
					accounts[0],
			});
		},
	};

	/**
	 * Handle the addition and removal of new wallets.
	 */
	onMount($store, () => {
		const walletsApi = getWallets();
		$actions.setWalletRegistered(getRegisteredWallets(preferredWallets, walletFilter));

		const unsubscribeFromRegister = walletsApi.on('register', () => {
			$actions.setWalletRegistered(getRegisteredWallets(preferredWallets, walletFilter));
		});

		const unsubscribeFromUnregister = walletsApi.on('unregister', (unregisteredWallet) => {
			$actions.setWalletUnregistered(
				getRegisteredWallets(preferredWallets, walletFilter),
				unregisteredWallet,
			);
		});

		return () => {
			unsubscribeFromRegister();
			unsubscribeFromUnregister();
		};
	});

	/**
	 *  Handle various changes in properties for a wallet.
	 */
	onMount($store, () => {
		let currentWalletChangeEvent: (() => void) | null = null;

		const unlisten = listenKeys($store, ['currentWallet'], ({ currentWallet }) => {
			currentWalletChangeEvent =
				currentWallet?.features['standard:events'].on('change', ({ accounts }) => {
					// TODO: We should handle features changing that might make the list of wallets
					// or even the current wallet incompatible with the dApp.
					if (accounts) {
						$actions.updateWalletAccounts(accounts);
					}
				}) ?? null;
		});

		return () => {
			unlisten();
			currentWalletChangeEvent?.();
		};
	});

	return {
		$store,
		$recentConnection,
		$actions,
	} as const;
}
