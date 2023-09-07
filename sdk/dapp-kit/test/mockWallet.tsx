// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import type { IdentifierRecord } from '@mysten/wallet-standard';
import { ReadonlyWalletAccount, SUI_CHAINS } from '@mysten/wallet-standard';
import type { Wallet } from '@mysten/wallet-standard';

export class MockWallet implements Wallet {
	version = '1.0.0' as const;
	icon = `data:image/png;base64,` as const;
	chains = SUI_CHAINS;
	#walletName: string;
	#additionalFeatures: IdentifierRecord<unknown>;

	#connect = vi.fn().mockReturnValue({ accounts: this.accounts });
	#disconnect = vi.fn();
	#on = vi.fn();
	#signPersonalMessage = vi.fn();
	#signTransactionBlock = vi.fn();
	#signAndExecuteTransactionBlock = vi.fn();

	constructor(name: string, additionalFeatures: IdentifierRecord<unknown>) {
		this.#walletName = name;
		this.#additionalFeatures = additionalFeatures;
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

	get features(): IdentifierRecord<unknown> {
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
			...this.#additionalFeatures,
		};
	}
}
