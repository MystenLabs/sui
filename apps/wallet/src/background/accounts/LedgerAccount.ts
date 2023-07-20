// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import {
	type PasswordUnLockableAccount,
	type SerializedAccount,
	type SerializedUIAccount,
	Account,
} from './Account';
import { decrypt, encrypt } from '_src/shared/cryptography/keystore';

export interface LedgerAccountSerialized extends SerializedAccount {
	type: 'ledger';
	derivationPath: string;
	// just used for authentication nothing is stored here at the moment
	encrypted: string;
}

export interface LedgerAccountSerializedUI extends SerializedUIAccount {
	type: 'ledger';
	derivationPath: string;
}

export function isLedgerAccountSerializedUI(
	account: SerializedUIAccount,
): account is LedgerAccountSerializedUI {
	return account.type === 'ledger';
}

export class LedgerAccount
	extends Account<LedgerAccountSerialized, null>
	implements PasswordUnLockableAccount
{
	readonly unlockType = 'password';

	static async createNew({
		address,
		publicKey,
		password,
		derivationPath,
	}: {
		address: SuiAddress;
		publicKey: string | null;
		password: string;
		derivationPath: string;
	}): Promise<Omit<LedgerAccountSerialized, 'id'>> {
		return {
			storageEntityType: 'account-entity',
			type: 'ledger',
			address,
			publicKey,
			encrypted: await encrypt(password, {}),
			derivationPath,
		};
	}

	static isOfType(serialized: SerializedAccount): serialized is LedgerAccountSerialized {
		return serialized.type === 'ledger';
	}

	constructor({ id }: { id: string }) {
		super({ type: 'ledger', id });
	}

	lock(): Promise<void> {
		return Promise.resolve();
	}

	isLocked(): Promise<boolean> {
		return Promise.resolve(false);
	}

	async toUISerialized(): Promise<LedgerAccountSerializedUI> {
		const { address, type, publicKey, derivationPath } = await this.getStoredData();
		return {
			id: this.id,
			type,
			address,
			isLocked: false,
			publicKey,
			derivationPath,
		};
	}

	async passwordUnlock(password: string): Promise<void> {
		const { encrypted } = await this.getStoredData();
		await decrypt<string>(password, encrypted);
	}
}
