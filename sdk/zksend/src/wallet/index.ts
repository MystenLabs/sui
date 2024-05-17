// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui/bcs';
import { Transaction } from '@mysten/sui/transactions';
import { toB64 } from '@mysten/sui/utils';
import type {
	StandardConnectFeature,
	StandardConnectMethod,
	StandardDisconnectFeature,
	StandardDisconnectMethod,
	StandardEventsFeature,
	StandardEventsListeners,
	StandardEventsOnMethod,
	SuiSignPersonalMessageFeature,
	SuiSignPersonalMessageMethod,
	SuiSignTransactionBlockFeature,
	SuiSignTransactionBlockMethod,
	SuiSignTransactionFeature,
	SuiSignTransactionMethod,
	Wallet,
} from '@mysten/wallet-standard';
import { getWallets, ReadonlyWalletAccount, SUI_MAINNET_CHAIN } from '@mysten/wallet-standard';
import type { Emitter } from 'mitt';
import mitt from 'mitt';

import { DEFAULT_STASHED_ORIGIN, StashedPopup } from './channel/index.js';

type WalletEventsMap = {
	[E in keyof StandardEventsListeners]: Parameters<StandardEventsListeners[E]>[0];
};

const STASHED_RECENT_ADDRESS_KEY = 'stashed:recentAddress';

export const STASHED_WALLET_NAME = 'Stashed' as const;

export class StashedWallet implements Wallet {
	#events: Emitter<WalletEventsMap>;
	#accounts: ReadonlyWalletAccount[];
	#origin: string;
	#name: string;

	get name() {
		return STASHED_WALLET_NAME;
	}

	get icon() {
		return 'data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNTYiIGhlaWdodD0iNTYiIHZpZXdCb3g9IjAgMCA1NiA1NiIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj4KPHJlY3QgeD0iMSIgeT0iMSIgd2lkdGg9IjU0IiBoZWlnaHQ9IjU0IiByeD0iMjciIHN0cm9rZT0iYmxhY2siIHN0cm9rZS13aWR0aD0iMiIvPgo8cGF0aCBkPSJNMTguMzUyOCAzNS4wNjM4QzE5LjI3NDIgMzguNTAyNSAyMi43MTU3IDQxLjYxNTQgMjkuODM2MSAzOS43MDc1QzM2LjYzMDEgMzcuODg3IDQwLjg4OCAzMi4yOTggMzkuNzgzOSAyOC4xNzc2QzM5LjQwMjYgMjYuNzU0NyAzOC4yNTQyIDI1Ljc5MTUgMzYuNDgzNyAyNS45NDgyTDIwLjY1MTkgMjcuMjY3M0MxOS42NTQ4IDI3LjM0MzggMTkuMTk3NiAyNy4xODA0IDE4LjkzMzkgMjYuNTUyMUMxOC42NzgxIDI1Ljk1MzQgMTguODIzOCAyNS4zMTA3IDIwLjAyODEgMjQuNzAyTDMyLjA3NjQgMTguNTE4OUMzMi45OTk4IDE4LjA0OTEgMzMuNjE0OSAxNy44NTI1IDM0LjE3NyAxOC4wNTE0QzM0LjUyOTIgMTguMTc5NCAzNC43NjEyIDE4LjY4OTEgMzQuNTQ4MiAxOS4zMTgxTDMzLjc2NyAyMS42MjQ0QzMyLjgwODMgMjQuNDU0OCAzNC44NjA2IDI1LjExMjIgMzYuMDE3NiAyNC44MDIyQzM3Ljc2ODEgMjQuMzMzMiAzOC4xNzk5IDIyLjY2NiAzNy42MTU5IDIwLjU2MTNDMzYuMTg2MiAxNS4yMjU0IDMwLjUyNTIgMTQuMzkxMiAyNS4zOTI2IDE1Ljc2NjRDMjAuMTcxIDE3LjE2NTYgMTUuNjQ0NiAyMS4zOTY3IDE3LjAyNjcgMjYuNTU0N0MxNy4zNTI0IDI3Ljc3MDEgMTguNDcxMSAyOC43NDEyIDE5Ljc2NyAyOC43MTE3TDIxLjc0NTIgMjguNzA2OUMyMi4xNTIxIDI4LjY5NzUgMjIuMDA1NiAyOC43MzA5IDIyLjc5MDUgMjguNjY1OUMyMy41NzUzIDI4LjYwMDkgMjUuNjcxNSAyOC4zNDMgMjUuNjcxNSAyOC4zNDNMMzUuOTU3MiAyNy4xNzlMMzYuMjIyMiAyNy4xNDA1QzM2LjgyMzcgMjcuMDM3OSAzNy4yNzgzIDI3LjE5NDIgMzcuNjYyNyAyNy44NTYzQzM4LjIzNzkgMjguODQ3MSAzNy4zNjAzIDI5LjU5NDMgMzYuMzA5OCAzMC40ODg4QzM2LjI4MTcgMzAuNTEyNyAzNi4yNTM1IDMwLjUzNjcgMzYuMjI1MSAzMC41NjA5TDI3LjE4MzcgMzguMzUzQzI1LjYzMzkgMzkuNjg5NiAyNC41NDUzIDM5LjE4NyAyNC4xNjQgMzcuNzY0MUwyMi44MTM3IDMyLjcyNDdDMjIuNDgwMSAzMS40Nzk3IDIxLjI2NDQgMzAuNTAyOCAxOS44NDAzIDMwLjg4NDRDMTguMDYwMiAzMS4zNjEzIDE3LjkxNTkgMzMuNDMzNCAxOC4zNTI4IDM1LjA2MzhaIiBmaWxsPSJibGFjayIvPgo8L3N2Zz4K' as const;
	}

	get version() {
		return '1.0.0' as const;
	}

	get chains() {
		return [SUI_MAINNET_CHAIN] as const;
	}

	get accounts() {
		return this.#accounts;
	}

	get features(): StandardConnectFeature &
		StandardDisconnectFeature &
		StandardEventsFeature &
		SuiSignTransactionBlockFeature &
		SuiSignTransactionFeature &
		SuiSignPersonalMessageFeature {
		return {
			'standard:connect': {
				version: '1.0.0',
				connect: this.#connect,
			},
			'standard:disconnect': {
				version: '1.0.0',
				disconnect: this.#disconnect,
			},
			'standard:events': {
				version: '1.0.0',
				on: this.#on,
			},
			'sui:signTransactionBlock': {
				version: '1.0.0',
				signTransactionBlock: this.#signTransactionBlock,
			},
			'sui:signTransaction': {
				version: '2.0.0',
				signTransaction: this.#signTransaction,
			},
			'sui:signPersonalMessage': {
				version: '1.0.0',
				signPersonalMessage: this.#signPersonalMessage,
			},
		};
	}

	constructor({
		name,
		address,
		origin = DEFAULT_STASHED_ORIGIN,
	}: {
		origin?: string;
		address?: string | null;
		name: string;
	}) {
		this.#accounts = [];
		this.#events = mitt();
		this.#origin = origin;
		this.#name = name;

		if (address) {
			this.#setAccount(address);
		}
	}

	#signTransactionBlock: SuiSignTransactionBlockMethod = async ({ transactionBlock, account }) => {
		transactionBlock.setSenderIfNotSet(account.address);

		const data = transactionBlock.serialize();

		const popup = new StashedPopup({
			name: this.#name,
			origin: this.#origin,
		});

		const response = await popup.send({
			type: 'sign-transaction-block',
			data,
			address: account.address,
		});

		return {
			transactionBlockBytes: response.bytes,
			signature: response.signature,
		};
	};

	#signTransaction: SuiSignTransactionMethod = async ({ transaction, account }) => {
		const popup = new StashedPopup({
			name: this.#name,
			origin: this.#origin,
		});

		const tx = Transaction.from(await transaction.toJSON());
		tx.setSenderIfNotSet(account.address);

		const data = tx.serialize();

		const response = await popup.send({
			type: 'sign-transaction-block',
			data,
			address: account.address,
		});

		return {
			bytes: response.bytes,
			signature: response.signature,
		};
	};

	#signPersonalMessage: SuiSignPersonalMessageMethod = async ({ message, account }) => {
		const bytes = toB64(bcs.vector(bcs.u8()).serialize(message).toBytes());
		const popup = new StashedPopup({
			name: this.#name,
			origin: this.#origin,
		});

		const response = await popup.send({
			type: 'sign-personal-message',
			bytes,
			address: account.address,
		});

		return {
			bytes,
			signature: response.signature,
		};
	};

	#on: StandardEventsOnMethod = (event, listener) => {
		this.#events.on(event, listener);
		return () => this.#events.off(event, listener);
	};

	#setAccount(address?: string) {
		if (address) {
			this.#accounts = [
				new ReadonlyWalletAccount({
					address,
					chains: [SUI_MAINNET_CHAIN],
					features: ['sui:signTransactionBlock', 'sui:signPersonalMessage'],
					// NOTE: Stashed doesn't support getting public keys, and zkLogin accounts don't have meaningful public keys anyway
					publicKey: new Uint8Array(),
				}),
			];

			localStorage.setItem(STASHED_RECENT_ADDRESS_KEY, address);
		} else {
			this.#accounts = [];
		}

		this.#events.emit('change', { accounts: this.accounts });
	}

	#connect: StandardConnectMethod = async (input) => {
		if (input?.silent) {
			const address = localStorage.getItem(STASHED_RECENT_ADDRESS_KEY);

			if (address) {
				this.#setAccount(address);
			}

			return { accounts: this.accounts };
		}

		const popup = new StashedPopup({ name: this.#name, origin: this.#origin });

		const response = await popup.send({
			type: 'connect',
		});

		if (!('address' in response)) {
			throw new Error('Unexpected response');
		}

		this.#setAccount(response.address);

		return { accounts: this.accounts };
	};

	#disconnect: StandardDisconnectMethod = async () => {
		localStorage.removeItem(STASHED_RECENT_ADDRESS_KEY);
		this.#setAccount();
	};
}

export function registerStashedWallet(
	name: string,
	{
		origin,
	}: {
		origin?: string;
	},
) {
	const wallets = getWallets();

	let addressFromRedirect: string | null = null;
	try {
		const params = new URLSearchParams(window.location.search);
		addressFromRedirect = params.get('stashed_address') || params.get('zksend_address');
	} catch {
		// Ignore errors
	}

	const wallet = new StashedWallet({
		name,
		origin,
		address: addressFromRedirect,
	});

	const unregister = wallets.register(wallet);

	return {
		wallet,
		unregister,
		addressFromRedirect,
	};
}
