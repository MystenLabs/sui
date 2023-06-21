// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { WalletAdapter, WalletAdapterEvents } from '@mysten/wallet-adapter-base';
import {
	StandardWalletAdapterWallet,
	SuiSignAndExecuteTransactionBlockVersion,
	SuiSignTransactionBlockVersion,
} from '@mysten/wallet-standard';
import mitt from 'mitt';

export interface StandardWalletAdapterConfig {
	wallet: StandardWalletAdapterWallet;
}

type WalletAdapterEventsMap = {
	[E in keyof WalletAdapterEvents]: Parameters<WalletAdapterEvents[E]>[0];
};

const suiSignTransactionBlockLatestVersion: SuiSignTransactionBlockVersion = '1.0.0';
const suiSignAndExecuteTransactionBlockLatestVersion: SuiSignAndExecuteTransactionBlockVersion =
	'1.0.0';

function isFeatureCompatible(featureVersion: string, adapterVersion: string) {
	const [featureMajor] = featureVersion.split('.');
	const [adapterMajor] = adapterVersion.split('.');
	return Number(adapterMajor) === Number(featureMajor);
}

export class StandardWalletAdapter implements WalletAdapter {
	connected = false;
	connecting = false;

	readonly #events = mitt<WalletAdapterEventsMap>();
	#wallet: StandardWalletAdapterWallet;
	#walletEventUnsubscribe: (() => void) | null = null;

	constructor({ wallet }: StandardWalletAdapterConfig) {
		this.#wallet = wallet;
	}

	get name() {
		return this.#wallet.name;
	}

	get icon() {
		return this.#wallet.icon;
	}

	get wallet() {
		return this.#wallet;
	}

	async getAccounts() {
		return this.#wallet.accounts;
	}

	async connect() {
		try {
			if (this.connected || this.connecting) return;
			this.connecting = true;

			this.#walletEventUnsubscribe = this.#wallet.features['standard:events'].on(
				'change',
				async ({ accounts }) => {
					if (accounts) {
						this.connected = accounts.length > 0;
						await this.#notifyChanged();
					}
				},
			);

			if (!this.#wallet.accounts.length) {
				await this.#wallet.features['standard:connect'].connect();
			}

			if (!this.#wallet.accounts.length) {
				throw new Error('No wallet accounts found');
			}

			this.connected = true;
			await this.#notifyChanged();
		} finally {
			this.connecting = false;
		}
	}

	async disconnect() {
		if (this.#wallet.features['standard:disconnect']) {
			await this.#wallet.features['standard:disconnect'].disconnect();
		}
		this.connected = false;
		this.connecting = false;
		if (this.#walletEventUnsubscribe) {
			this.#walletEventUnsubscribe();
			this.#walletEventUnsubscribe = null;
		}
	}

	signMessage: WalletAdapter['signMessage'] = (messageInput) => {
		return this.#wallet.features['sui:signMessage'].signMessage(messageInput);
	};

	signTransactionBlock: WalletAdapter['signTransactionBlock'] = (transactionInput) => {
		const version = this.#wallet.features['sui:signTransactionBlock'].version;
		if (!isFeatureCompatible(version, suiSignTransactionBlockLatestVersion)) {
			throw new Error(
				`Version mismatch, signTransaction feature version ${version} is not compatible with version ${suiSignTransactionBlockLatestVersion}`,
			);
		}
		return this.#wallet.features['sui:signTransactionBlock'].signTransactionBlock(transactionInput);
	};

	signAndExecuteTransactionBlock: WalletAdapter['signAndExecuteTransactionBlock'] = (
		transactionInput,
	) => {
		const version = this.#wallet.features['sui:signAndExecuteTransactionBlock'].version;
		if (!isFeatureCompatible(version, suiSignAndExecuteTransactionBlockLatestVersion)) {
			throw new Error(
				`Version mismatch, signAndExecuteTransactionBlock feature version ${version} is not compatible with version ${suiSignAndExecuteTransactionBlockLatestVersion}`,
			);
		}
		return this.#wallet.features[
			'sui:signAndExecuteTransactionBlock'
		].signAndExecuteTransactionBlock(transactionInput);
	};

	on: <E extends keyof WalletAdapterEvents>(
		event: E,
		callback: WalletAdapterEvents[E],
	) => () => void = (event, callback) => {
		this.#events.on(event, callback);
		return () => {
			this.#events.off(event, callback);
		};
	};

	async #notifyChanged() {
		this.#events.emit('change', {
			connected: this.connected,
			accounts: await this.getAccounts(),
		});
	}
}
