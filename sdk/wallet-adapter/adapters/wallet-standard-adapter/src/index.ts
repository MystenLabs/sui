// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { WalletAdapterProvider } from '@mysten/wallet-adapter-base';
import {
	isStandardWalletAdapterCompatibleWallet,
	StandardWalletAdapterWallet,
	Wallets,
	getWallets,
} from '@mysten/wallet-standard';
import { StandardWalletAdapter } from './StandardWalletAdapter';
import mitt, { Emitter } from 'mitt';

type Events = {
	changed: void;
};

export { StandardWalletAdapter };

// These are the default features that the adapter will check for:
export const DEFAULT_FEATURES: (keyof StandardWalletAdapterWallet['features'])[] = [
	'sui:signAndExecuteTransactionBlock',
];

export class WalletStandardAdapterProvider implements WalletAdapterProvider {
	#wallets: Wallets;
	#adapters: Map<StandardWalletAdapterWallet, StandardWalletAdapter>;
	#events: Emitter<Events>;
	#features: string[];

	constructor({ features }: { features?: string[] } = {}) {
		this.#adapters = new Map();
		this.#wallets = getWallets();
		this.#events = mitt();
		this.#features = features ?? DEFAULT_FEATURES;

		this.#wallets.on('register', () => {
			this.#events.emit('changed');
		});

		this.#wallets.on('unregister', () => {
			this.#events.emit('changed');
		});
	}

	get() {
		const filtered = this.#wallets
			.get()
			.filter((wallet) =>
				isStandardWalletAdapterCompatibleWallet(wallet, this.#features),
			) as StandardWalletAdapterWallet[];

		filtered.forEach((wallet) => {
			if (!this.#adapters.has(wallet)) {
				this.#adapters.set(wallet, new StandardWalletAdapter({ wallet }));
			}
		});

		return [...this.#adapters.values()];
	}

	on<T extends keyof Events>(eventName: T, callback: (data: Events[T]) => void) {
		this.#events.on(eventName, callback);
		return () => {
			this.#events.off(eventName, callback);
		};
	}
}
