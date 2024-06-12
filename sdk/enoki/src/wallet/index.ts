// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
	SuiSignTransactionFeature,
	SuiSignTransactionMethod,
	Wallet,
} from '@mysten/wallet-standard';
import {
	getWallets,
	ReadonlyWalletAccount,
	SUI_DEVNET_CHAIN,
	SUI_MAINNET_CHAIN,
	SUI_TESTNET_CHAIN,
} from '@mysten/wallet-standard';
import type { Emitter } from 'mitt';
import mitt from 'mitt';

import type { EnokiFlowConfig } from '../EnokiFlow.js';
import { EnokiFlow } from '../EnokiFlow.js';

type WalletEventsMap = {
	[E in keyof StandardEventsListeners]: Parameters<StandardEventsListeners[E]>[0];
};

const ENOKI_PROVIDER_WALLETS_INFO = {
	google: {
		name: 'Google',
		icon: 'data:image/svg+xml;base64,PHN2ZyBmaWxsPSJub25lIiBoZWlnaHQ9IjMyIiB2aWV3Qm94PSIwIDAgMzIgMzIiIHdpZHRoPSIzMiIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48cGF0aCBkPSJtMzIgMGgtMzJ2MzJoMzJ6IiBmaWxsPSIjZmZmIi8+PGcgY2xpcC1ydWxlPSJldmVub2RkIiBmaWxsLXJ1bGU9ImV2ZW5vZGQiPjxwYXRoIGQ9Im0yMy44Mjk5IDE2LjE4MThjMC0uNTY3Mi0uMDUwOS0xLjExMjctLjE0NTQtMS42MzYzaC03LjUzNDZ2My4wOTQ1aDQuMzA1NWMtLjE4NTUgMS0uNzQ5MSAxLjg0NzMtMS41OTY0IDIuNDE0NnYyLjAwNzNoMi41ODU1YzEuNTEyNy0xLjM5MjggMi4zODU0LTMuNDQzNyAyLjM4NTQtNS44ODAxeiIgZmlsbD0iIzQyODVmNCIvPjxwYXRoIGQ9Im0xNi4xNDk2IDI0YzIuMTYgMCAzLjk3MDktLjcxNjQgNS4yOTQ2LTEuOTM4MmwtMi41ODU1LTIuMDA3M2MtLjcxNjQuNDgtMS42MzI3Ljc2MzYtMi43MDkxLjc2MzYtMi4wODM2IDAtMy44NDczLTEuNDA3Mi00LjQ3NjQtMy4yOTgxaC0yLjY3MjcxdjIuMDcyN2MxLjMxNjQxIDIuNjE0NSA0LjAyMTgxIDQuNDA3MyA3LjE0OTExIDQuNDA3M3oiIGZpbGw9IiMzNGE4NTMiLz48cGF0aCBkPSJtMTEuNjczNSAxNy41MmMtLjE2LS40OC0uMjUwOS0uOTkyOC0uMjUwOS0xLjUyIDAtLjUyNzMuMDkwOS0xLjA0LjI1MDktMS41MnYtMi4wNzI4aC0yLjY3MjY5Yy0uNTQxODIgMS4wOC0uODUwOTEgMi4zMDE4LS44NTA5MSAzLjU5MjggMCAxLjI5MDkuMzA5MDkgMi41MTI3Ljg1MDkxIDMuNTkyN3oiIGZpbGw9IiNmYmJjMDUiLz48cGF0aCBkPSJtMTYuMTQ5NiAxMS4xODE4YzEuMTc0NSAwIDIuMjI5MS40MDM3IDMuMDU4MiAxLjE5NjRsMi4yOTQ1LTIuMjk0NmMtMS4zODU0LTEuMjkwODctMy4xOTYzLTIuMDgzNi01LjM1MjctMi4wODM2LTMuMTI3MyAwLTUuODMyNyAxLjc5MjczLTcuMTQ5MTEgNC40MDczbDIuNjcyNzEgMi4wNzI3Yy42MjkxLTEuODkwOSAyLjM5MjgtMy4yOTgyIDQuNDc2NC0zLjI5ODJ6IiBmaWxsPSIjZWE0MzM1Ii8+PC9nPjwvc3ZnPg==',
	},
} as const;

export class EnokiWallet implements Wallet {
	#events: Emitter<WalletEventsMap>;
	#accounts: ReadonlyWalletAccount[];
	#name: string;
	#icon: Wallet['icon'];
	#flow: EnokiFlow;

	get name() {
		return this.#name;
	}

	get icon() {
		return this.#icon;
	}

	get version() {
		return '1.0.0' as const;
	}

	get chains() {
		return [SUI_MAINNET_CHAIN, SUI_TESTNET_CHAIN, SUI_DEVNET_CHAIN] as const;
	}

	get accounts() {
		return this.#accounts;
	}

	get features(): StandardConnectFeature &
		StandardDisconnectFeature &
		StandardEventsFeature &
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

	constructor({ name, icon, flow }: { icon: Wallet['icon']; name: string; flow: EnokiFlow }) {
		this.#accounts = [];
		this.#events = mitt();

		this.#name = name;
		this.#icon = icon;
		this.#flow = flow;
	}

	#signTransaction: SuiSignTransactionMethod = async ({ transaction, account }) => {
		throw new Error('Not implemented');
	};

	#signPersonalMessage: SuiSignPersonalMessageMethod = async ({ message, account }) => {
		throw new Error('Not implemented');
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
					features: ['sui:signTransaction', 'sui:signPersonalMessage'],
					// NOTE: Stashed doesn't support getting public keys, and zkLogin accounts don't have meaningful public keys anyway
					publicKey: new Uint8Array(),
				}),
			];
		} else {
			this.#accounts = [];
		}

		this.#events.emit('change', { accounts: this.accounts });
	}

	#connect: StandardConnectMethod = async (input) => {
		throw new Error('Not implemented');

		// this.#setAccount(response.address);

		return { accounts: this.accounts };
	};

	#disconnect: StandardDisconnectMethod = async () => {
		this.#setAccount();
	};
}

export function registerEnokidWallets(config: EnokiFlowConfig) {
	const walletsApi = getWallets();
	const flow = new EnokiFlow(config);

	const wallets: {
		unregister: () => void;
		wallet: EnokiWallet;
	}[] = [];

	for (const { name, icon } of Object.values(ENOKI_PROVIDER_WALLETS_INFO)) {
		const wallet = new EnokiWallet({
			name,
			icon,
			flow,
		});

		const unregister = walletsApi.register(wallet);

		wallets.push({ wallet, unregister });
	}

	return {
		wallets: wallets.map(({ wallet }) => wallet),
		unregister: () => {
			for (const { unregister } of wallets) {
				unregister();
			}
		},
	};
}
