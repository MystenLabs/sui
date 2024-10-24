// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui/client';
import { getWallets } from '@mysten/wallet-standard';
import { atom, computed, listenKeys, onMount, task } from 'nanostores';

import { getRegisteredWallets, getWalletUniqueIdentifier } from '../../utils/walletUtils.js';
import { createMethods } from './methods.js';
import type { DappKitStateOptions } from './state.js';
import { createState } from './state.js';

export type DappKitStore = ReturnType<typeof createDappKitStore>;

type CreateDappKitStoreOptions = DappKitStateOptions & {
	client: SuiClient;
};

export function createDappKitStore(options: CreateDappKitStoreOptions) {
	const $client = atom(options.client);
	const { $state, $recentConnection, actions } = createState(options);
	const methods = createMethods({ $state, actions, $client });

	/**
	 * Handle the addition and removal of new wallets.
	 */
	onMount($state, () => {
		const { preferredWallets = [], walletFilter } = options;

		const walletsApi = getWallets();
		actions.setWalletRegistered(getRegisteredWallets(preferredWallets, walletFilter));

		const unsubscribeFromRegister = walletsApi.on('register', () => {
			actions.setWalletRegistered(getRegisteredWallets(preferredWallets, walletFilter));
		});

		const unsubscribeFromUnregister = walletsApi.on('unregister', (unregisteredWallet) => {
			actions.setWalletUnregistered(
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
	onMount($state, () => {
		let currentWalletChangeEvent: (() => void) | null = null;

		const unlisten = listenKeys($state, ['currentWallet'], ({ currentWallet }) => {
			currentWalletChangeEvent =
				currentWallet?.features['standard:events'].on('change', ({ accounts }) => {
					// TODO: We should handle features changing that might make the list of wallets
					// or even the current wallet incompatible with the dApp.
					if (accounts) {
						actions.updateWalletAccounts(accounts);
					}
				}) ?? null;
		});

		return () => {
			unlisten();
			currentWalletChangeEvent?.();
		};
	});

	// Auto-connect wallet:
	if (options.autoConnectEnabled) {
		onMount($state, () => {
			task(async () => {
				const { wallets, connectionStatus } = $state.get();
				const { walletName, accountAddress } = $recentConnection.get();
				const wallet = wallets.find((wallet) => getWalletUniqueIdentifier(wallet) === walletName);

				if (!walletName || !accountAddress || !wallet || connectionStatus === 'connected') {
					$state.setKey('autoConnectStatus', 'attempted');
					return;
				}

				try {
					await methods.connectWallet({
						wallet,
						accountAddress,
						silent: true,
					});
				} catch {
					// Ignore errors:
				} finally {
					$state.setKey('autoConnectStatus', 'attempted');
				}
			});
		});
	}

	return {
		...methods,

		atoms: {
			$client,

			// Wallet state:
			$autoConnectStatus: computed($state, (state) => state.autoConnectStatus),
			$wallets: computed($state, (state) => state.wallets),
			$accounts: computed($state, (state) => state.accounts),
			$currentAccount: computed($state, (state) => state.currentAccount),
			$currentWallet: computed($state, ({ currentWallet, connectionStatus, supportedIntents }) => {
				switch (connectionStatus) {
					case 'connecting':
						return {
							connectionStatus,
							currentWallet: null,
							isDisconnected: false,
							isConnecting: true,
							isConnected: false,
							supportedIntents: [],
						} as const;
					case 'disconnected':
						return {
							connectionStatus,
							currentWallet: null,
							isDisconnected: true,
							isConnecting: false,
							isConnected: false,
							supportedIntents: [],
						} as const;
					case 'connected': {
						return {
							connectionStatus,
							currentWallet: currentWallet!,
							isDisconnected: false,
							isConnecting: false,
							isConnected: true,
							supportedIntents,
						} as const;
					}
				}
			}),
		},
	} as const;
}
