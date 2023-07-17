// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress, type SuiAddress } from '@mysten/sui.js';

import { type Account, AccountType } from './Account';

export type SerializedLedgerAccount = {
	type: AccountType.LEDGER;
	address: SuiAddress;
	derivationPath: string;
	publicKey: string | null;
};

export class LedgerAccount implements Account {
	readonly type: AccountType;
	readonly address: SuiAddress;
	readonly derivationPath: string;
	#publicKey: string | null;

	constructor({
		address,
		derivationPath,
		publicKey,
	}: {
		address: SuiAddress;
		derivationPath: string;
		publicKey: string | null;
	}) {
		this.type = AccountType.LEDGER;
		this.address = normalizeSuiAddress(address);
		this.derivationPath = derivationPath;
		this.#publicKey = publicKey;
	}

	toJSON(): SerializedLedgerAccount {
		return {
			type: AccountType.LEDGER,
			address: this.address,
			derivationPath: this.derivationPath,
			publicKey: this.#publicKey,
		};
	}

	getPublicKey() {
		return this.#publicKey;
	}

	setPublicKey(publicKey: string) {
		this.#publicKey = publicKey;
	}
}
