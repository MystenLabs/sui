// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress } from '@mysten/sui.js/utils';

import { type Account, AccountType } from './Account';
import { type Wallet } from '_src/shared/qredo-api';

export type SerializedQredoAccount = {
	type: AccountType.QREDO;
	address: string;
	qredoConnectionID: string;
	qredoWalletID: string;
	labels: Wallet['labels'];
	derivationPath: null;
	publicKey: string;
	network: string;
	walletID: string;
};

export class QredoAccount implements Account {
	readonly type = AccountType.QREDO;
	readonly address: string;
	readonly qredoConnectionID: string;
	readonly qredoWalletID: string;
	readonly labels: Wallet['labels'];
	readonly publicKey: string;
	readonly network: string;
	readonly walletID: string;

	constructor({
		address,
		qredoConnectionID,
		qredoWalletID,
		labels = [],
		publicKey,
		network,
		walletID,
	}: Omit<SerializedQredoAccount, 'type' | 'derivationPath'>) {
		this.address = normalizeSuiAddress(address);
		this.qredoConnectionID = qredoConnectionID;
		this.qredoWalletID = qredoWalletID;
		this.labels = labels;
		this.publicKey = publicKey;
		this.network = network;
		this.walletID = walletID;
	}

	toJSON(): SerializedQredoAccount {
		return {
			type: this.type,
			address: this.address,
			qredoConnectionID: this.qredoConnectionID,
			qredoWalletID: this.qredoWalletID,
			labels: this.labels,
			derivationPath: null,
			publicKey: this.publicKey,
			network: this.network,
			walletID: this.walletID,
		};
	}

	getPublicKey() {
		return this.publicKey;
	}
}
