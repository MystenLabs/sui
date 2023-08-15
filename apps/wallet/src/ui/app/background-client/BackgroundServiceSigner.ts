// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiClient } from '@mysten/sui.js/client';
import { WalletSigner } from '../WalletSigner';

import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { type SerializedAccount } from '_src/background/keyring/Account';
import type { BackgroundClient } from '.';
import type { SerializedSignature } from '@mysten/sui.js/cryptography';

export class BackgroundServiceSigner extends WalletSigner {
	readonly #account: SerializedAccount | SerializedUIAccount;
	readonly #backgroundClient: BackgroundClient;

	constructor(
		account: SerializedAccount | SerializedUIAccount,
		backgroundClient: BackgroundClient,
		client: SuiClient,
	) {
		super(client);
		this.#account = account;
		this.#backgroundClient = backgroundClient;
	}

	async getAddress(): Promise<string> {
		return this.#account.address;
	}

	signData(data: Uint8Array): Promise<SerializedSignature> {
		return this.#backgroundClient.signData(
			'id' in this.#account ? this.#account.id : this.#account.address,
			data,
		);
	}

	connect(client: SuiClient) {
		return new BackgroundServiceSigner(this.#account, this.#backgroundClient, client);
	}
}
