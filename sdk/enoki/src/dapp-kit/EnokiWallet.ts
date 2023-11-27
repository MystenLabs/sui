// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	StandardConnectFeature,
	StandardConnectMethod,
	StandardEventsFeature,
	StandardEventsOnMethod,
	SuiFeatures,
	SuiSignAndExecuteTransactionBlockMethod,
	SuiSignPersonalMessageMethod,
	SuiSignTransactionBlockMethod,
	Wallet,
} from '@mysten/wallet-standard';
import { getWallets, SUI_CHAINS } from '@mysten/wallet-standard';

import type { EnokiClient } from '../EnokiClient/index.js';
import type { AuthProvider } from '../EnokiFlow.js';

const getWalletName = (provider: AuthProvider, clientId: string) =>
	`Enoki Wallet (${provider} ${clientId})`;

export type EnokiFeature = {
	readonly 'enoki:': {
		readonly version: '1.0.0';
	};
};

export class EnokiWallet implements Wallet {
	#client: EnokiClient;

	provider: AuthProvider;
	clientId: string;

	constructor(client: EnokiClient, provider: AuthProvider, clientId: string) {
		this.#client = client;
		this.provider = provider;
		this.clientId = clientId;
	}

	get version() {
		return '1.0.0' as const;
	}

	get name() {
		return getWalletName(this.provider, this.clientId);
	}

	// TODO:
	get icon() {
		return 'data:image/svg+xml;base64,TODO' as const;
	}

	get chains() {
		return SUI_CHAINS;
	}

	get accounts() {
		return [];
	}

	get features(): StandardConnectFeature & StandardEventsFeature & SuiFeatures & EnokiFeature {
		return {
			'enoki:': {
				version: '1.0.0',
			},
			'standard:connect': {
				version: '1.0.0',
				connect: this.#connect,
			},
			'standard:events': {
				version: '1.0.0',
				on: this.#on,
			},
			'sui:signPersonalMessage': {
				version: '1.0.0',
				signPersonalMessage: this.#signPersonalMessage,
			},
			'sui:signTransactionBlock': {
				version: '1.0.0',
				signTransactionBlock: this.#signTransactionBlock,
			},
			'sui:signAndExecuteTransactionBlock': {
				version: '1.0.0',
				signAndExecuteTransactionBlock: this.#signAndExecuteTransactionBlock,
			},
		};
	}

	#on: StandardEventsOnMethod = () => {
		throw new Error('Not yet implemented');
	};

	#connect: StandardConnectMethod = () => {
		throw new Error('Not yet implemented');
	};

	#signPersonalMessage: SuiSignPersonalMessageMethod = () => {
		throw new Error('Not yet implemented');
	};

	#signTransactionBlock: SuiSignTransactionBlockMethod = () => {
		throw new Error('Not yet implemented');
	};

	#signAndExecuteTransactionBlock: SuiSignAndExecuteTransactionBlockMethod = () => {
		throw new Error('Not yet implemented');
	};
}

export function registerEnokiWallet(client: EnokiClient, provider: AuthProvider, clientId: string) {
	const walletsApi = getWallets();
	const registeredWallets = walletsApi.get();

	if (registeredWallets.find((wallet) => wallet.name === getWalletName(provider, clientId))) {
		console.warn(
			'registerUnsafeBurnerWallet: Unsafe Burner Wallet already registered, skipping duplicate registration.',
		);
		return;
	}

	return walletsApi.register(new EnokiWallet(client, provider, clientId));
}
