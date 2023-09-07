// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import type {
	StandardConnectFeature,
	StandardConnectMethod,
	StandardDisconnectFeature,
	StandardDisconnectMethod,
	StandardEventsFeature,
	StandardEventsOnMethod,
	SuiFeatures,
	SuiSignAndExecuteTransactionBlockMethod,
	SuiSignPersonalMessageMethod,
	SuiSignTransactionBlockMethod,
} from '@mysten/wallet-standard';
import { ReadonlyWalletAccount, SUI_CHAINS } from '@mysten/wallet-standard';
import type { Wallet } from '@mysten/wallet-standard';

export class MockWallet implements Wallet {
	version = '1.0.0' as const;
	icon = `data:image/png;base64,` as const;
	chains = SUI_CHAINS;
	#walletName: string;

	constructor(name: string) {
		this.#walletName = name;
	}

	get name() {
		return this.#walletName;
	}

	get accounts() {
		const keypair = new Ed25519Keypair();
		const account = new ReadonlyWalletAccount({
			address: keypair.getPublicKey().toSuiAddress(),
			publicKey: keypair.getPublicKey().toSuiBytes(),
			chains: ['sui:unknown'],
			features: ['sui:signAndExecuteTransactionBlock', 'sui:signTransactionBlock'],
		});
		return [account];
	}

	get features(): StandardConnectFeature &
		StandardDisconnectFeature &
		StandardEventsFeature &
		SuiFeatures {
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
		return () => {};
	};

	#connect: StandardConnectMethod = async () => {
		return new Promise((resolve) => setTimeout(() => resolve({ accounts: this.accounts }), 800));
	};

	#disconnect: StandardDisconnectMethod = async () => {
		return new Promise((resolve) => setTimeout(() => resolve(), 800));
	};

	#signPersonalMessage: SuiSignPersonalMessageMethod = async ({ message }) => {
		return new Promise((resolve) => {
			setTimeout(
				() =>
					resolve({
						bytes: `test-bytes-for-${message}`,
						signature: `test-signature-${message}`,
					}),
				300,
			);
		});
	};

	#signTransactionBlock: SuiSignTransactionBlockMethod = async () => {
		return new Promise((resolve) => {
			setTimeout(
				() =>
					resolve({
						transactionBlockBytes: 'test-bytes',
						signature: 'test-signature',
					}),
				500,
			);
		});
	};

	#signAndExecuteTransactionBlock: SuiSignAndExecuteTransactionBlockMethod = async () => {
		return new Promise((resolve) => {
			setTimeout(
				() =>
					resolve({
						balanceChanges: null,
						checkpoint: '123',
						confirmedLocalExecution: null,
						digest: 'ABC',
						effects: null,
						errors: [],
						events: null,
						objectChanges: null,
						rawTransaction: '',
						timestampMs: null,
						transaction: null,
					}),
				500,
			);
		});
	};
}
