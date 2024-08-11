// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { type SuiClient } from '@mysten/sui/client';

import type { BackgroundClient } from '.';
import { WalletSigner } from '../WalletSigner';

export class BackgroundServiceSigner extends WalletSigner {
	readonly #account: SerializedUIAccount;
	readonly #backgroundClient: BackgroundClient;

	constructor(account: SerializedUIAccount, backgroundClient: BackgroundClient, client: SuiClient) {
		super(client);
		this.#account = account;
		this.#backgroundClient = backgroundClient;
	}

	async getAddress(): Promise<string> {
		return this.#account.address;
	}

	signData(data: Uint8Array): Promise<string> {
		return this.#backgroundClient.signData(this.#account.id, data);
	}

	connect(client: SuiClient) {
		return new BackgroundServiceSigner(this.#account, this.#backgroundClient, client);
	}
}
