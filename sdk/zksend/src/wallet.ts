// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	getWallets,
	ReadonlyWalletAccount,
	StandardConnectFeature,
	StandardConnectMethod,
	StandardDisconnectFeature,
	StandardDisconnectMethod,
	StandardEventsFeature,
	StandardEventsListeners,
	StandardEventsOnMethod,
	SUI_MAINNET_CHAIN,
	Wallet,
} from '@mysten/wallet-standard';
import mitt, { Emitter } from 'mitt';

import { ZkSendPopup } from './channel';

type WalletEventsMap = {
	[E in keyof StandardEventsListeners]: Parameters<StandardEventsListeners[E]>[0];
};

const ZKSEND_RECENT_ADDRESS_KEY = 'zksend:recentAddress';

export class ZkSendWallet implements Wallet {
	#events: Emitter<WalletEventsMap>;
	#accounts: ReadonlyWalletAccount[];

	get name() {
		return 'zkSend';
	}

	get icon() {
		return 'data:image/svg+xml;base64,PHN2ZyBmaWxsPSJub25lIiBoZWlnaHQ9IjMyIiB2aWV3Qm94PSIwIDAgMzIgMzIiIHdpZHRoPSIzMiIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIiB4bWxuczp4bGluaz0iaHR0cDovL3d3dy53My5vcmcvMTk5OS94bGluayI+PGNsaXBQYXRoIGlkPSJhIj48cmVjdCBoZWlnaHQ9IjMyIiByeD0iMiIgd2lkdGg9IjMyIi8+PC9jbGlwUGF0aD48ZyBjbGlwLXBhdGg9InVybCgjYSkiPjxyZWN0IGZpbGw9IiNmZmYiIGhlaWdodD0iMzIiIHJ4PSIyIiB3aWR0aD0iMzIiLz48cGF0aCBkPSJtMCAwaDMydjMyaC0zMnoiIGZpbGw9IiNkNDA1NTEiLz48cGF0aCBkPSJtNS42NjgyNSAyNS4yNDkxYy0uNzgyNjMtLjc4MjctLjc4MDgxLTIuMDUyMS4wMDQwNi0yLjgzMjVsMTYuNjA1MjktMTYuNTEwNDdjLjc4MTctLjc3NzIzIDIuMDQ0OS0uNzc1NDMgMi44MjQzLjAwNDA0bC44Mzg3LjgzODYyYy43ODI1Ljc4MjUxLjc4MDggMi4wNTE3NC0uMDAzOCAyLjgzMjE4bC0xNi42MDE4OCAxNi41MTM4M2MtLjc4MTY1Ljc3NzUtMi4wNDUwOC43NzU4LTIuODI0NjYtLjAwMzd6bTUuNDQzMzUtMTUuOTExNjZjLTEuODA5NzIuMDUzNjctMi43NTM3MS0yLjEzMzA5LTEuNDczNDctMy40MTMzM2wuODM4MzctLjgzODMyYy4zNzUtLjM3NTA4Ljg4MzctLjU4NTc5IDEuNDE0Mi0uNTg1NzloMTMuNDc5N2MxLjEwNDYgMCAyIC44OTU0MyAyIDJ2MTMuNDc5N2MwIC41MzA1LS4yMTA3IDEuMDM5Mi0uNTg1OCAxLjQxNDJsLS44MjY5LjgyN2MtMS4yODE4IDEuMjgxOC0zLjQ3MDkuMzMzOS0zLjQxMzItMS40Nzc5bC4zMDY2LTkuNjI5OWMuMDM2Ny0xLjE1MjI3LS45MDU5LTIuMDk2OS0yLjA1ODMtMi4wNjI3M3oiIGZpbGw9IiNmZmYiLz48L2c+PC9zdmc+' as const;
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

	get features(): StandardConnectFeature & StandardDisconnectFeature & StandardEventsFeature {
		// & SuiSignTransactionBlockFeature
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
			// 'sui:signTransactionBlock': {
			// 	version: '1.0.0',
			// 	signTransactionBlock: this.#signTransactionBlock,
			// },
			// TODO: Support signing personal messages:
			// 'sui:signPersonalMessage': {
			// 	version: '1.0.0',
			// 	signPersonalMessage: this.#signPersonalMessage,
			// },
		};
	}

	constructor() {
		this.#accounts = [];
		this.#events = mitt();
	}

	// #signTransactionBlock = () => {};

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
					features: ['sui:signTransactionBlock'],
					// NOTE: zkSend doesn't support getting public keys, and zkLogin accounts don't have meaningful public keys anyway
					publicKey: new Uint8Array(),
				}),
			];

			localStorage.setItem(ZKSEND_RECENT_ADDRESS_KEY, address);
		} else {
			this.#accounts = [];
		}

		this.#events.emit('change', { accounts: this.accounts });
	}

	#connect: StandardConnectMethod = async (input) => {
		if (input?.silent) {
			const address = localStorage.getItem(ZKSEND_RECENT_ADDRESS_KEY);

			if (address) {
				this.#setAccount(address);
			}

			return { accounts: this.accounts };
		}

		const popup = new ZkSendPopup();
		const response = await popup.createRequest('connect', {});
		if (!('address' in response)) {
			throw new Error('Unexpected response');
		}

		this.#setAccount(response.address);

		return { accounts: this.accounts };
	};

	#disconnect: StandardDisconnectMethod = async () => {
		localStorage.removeItem(ZKSEND_RECENT_ADDRESS_KEY);
		this.#setAccount();
	};
}

export function registerZkSendWallet() {
	const wallets = getWallets();

	return wallets.register(new ZkSendWallet());
}
