// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PublicKey, SignatureScheme } from '@mysten/sui/cryptography';
import { Signer } from '@mysten/sui/cryptography';
import { Transaction } from '@mysten/sui/transactions';
import type { IdentifierString, WalletAccount, WalletWithFeatures } from '@wallet-standard/core';

import type { SuiWalletFeatures } from './features/index.js';
import { signPersonalMessage, signTransaction } from './wallet.js';

export class WalletSigner extends Signer {
	#wallet: WalletWithFeatures<Partial<SuiWalletFeatures>>;
	#account: WalletAccount;
	#chain: IdentifierString;

	constructor({
		wallet,
		account,
		chain,
	}: {
		wallet: WalletWithFeatures<Partial<SuiWalletFeatures>>;
		account: WalletAccount;
		chain: IdentifierString;
	}) {
		super();
		this.#wallet = wallet;
		this.#account = account;
		this.#chain = chain;
	}

	getKeyScheme(): SignatureScheme {
		throw new Error('Signature is unavailable for WalletSigner');
	}

	getPublicKey(): PublicKey {
		throw new Error('PublicKey is unavailable for WalletSigner');
	}

	sign(_data: Uint8Array): never {
		throw new Error(
			'WalletSigner does not support signing directly. Use signTransaction or signPersonalMessage instead',
		);
	}

	signData(_data: Uint8Array): never {
		throw new Error(
			'WalletSigner does not support signing directly. Use signTransaction or signPersonalMessage instead',
		);
	}

	async signTransaction(bytes: Uint8Array) {
		const transaction = Transaction.from(bytes);

		return signTransaction(this.#wallet, {
			transaction,
			account: this.#account,
			chain: this.#chain,
		});
	}

	async signPersonalMessage(bytes: Uint8Array) {
		return signPersonalMessage(this.#wallet, {
			message: bytes,
			account: this.#account,
		});
	}
}
