// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { computed, listenKeys, onMount } from 'nanostores';

import { getCurrentWallet } from '../wallet/getCurrentWallet.js';
import { createMethods } from './methods.js';
import type { DappKitStateOptions } from './state.js';
import { createState } from './state.js';

export type DappKitStore = ReturnType<typeof createDappKitStore>;

export function createDappKitStore(options: DappKitStateOptions) {
	const { $state, actions } = createState(options);
	const methods = createMethods({ $state, actions });

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

	return {
		atoms: {
			$wallets: computed($state, (state) => state.wallets),
			$accounts: computed($state, (state) => state.accounts),
			$currentAccount: computed($state, (state) => state.currentAccount),
			$currentWallet: computed($state, (state) => getCurrentWallet(state)),
		},

		...methods,
	} as const;
}
