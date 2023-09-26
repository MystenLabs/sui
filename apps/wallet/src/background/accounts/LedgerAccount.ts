// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { decrypt, encrypt } from '_src/shared/cryptography/keystore';

import {
	Account,
	type PasswordUnlockableAccount,
	type SerializedAccount,
	type SerializedUIAccount,
} from './Account';

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

type EphemeralData = {
	unlocked: true;
};

export class LedgerAccount
	extends Account<LedgerAccountSerialized, EphemeralData>
	implements PasswordUnlockableAccount
{
	readonly unlockType = 'password';

	static async createNew({
		address,
		publicKey,
		password,
		derivationPath,
	}: {
		address: string;
		publicKey: string | null;
		password: string;
		derivationPath: string;
	}): Promise<Omit<LedgerAccountSerialized, 'id'>> {
		return {
			type: 'ledger',
			address,
			publicKey,
			encrypted: await encrypt(password, {}),
			derivationPath,
			lastUnlockedOn: null,
			selected: false,
			nickname: null,
			createdAt: Date.now(),
		};
	}

	static isOfType(serialized: SerializedAccount): serialized is LedgerAccountSerialized {
		return serialized.type === 'ledger';
	}

	constructor({ id, cachedData }: { id: string; cachedData?: LedgerAccountSerialized }) {
		super({ type: 'ledger', id, cachedData });
	}

	async lock(allowRead = false): Promise<void> {
		await this.clearEphemeralValue();
		await this.onLocked(allowRead);
	}

	async isLocked(): Promise<boolean> {
		return !(await this.getEphemeralValue())?.unlocked;
	}

	async passwordUnlock(password?: string): Promise<void> {
		if (!password) {
			throw new Error('Missing password to unlock the account');
		}
		const { encrypted } = await this.getStoredData();
		await decrypt<string>(password, encrypted);
		await this.setEphemeralValue({ unlocked: true });
		await this.onUnlocked();
	}

	async verifyPassword(password: string): Promise<void> {
		const { encrypted } = await this.getStoredData();
		await decrypt<string>(password, encrypted);
	}

	async toUISerialized(): Promise<LedgerAccountSerializedUI> {
		const { address, type, publicKey, derivationPath, selected, nickname } =
			await this.getStoredData();
		return {
			id: this.id,
			type,
			address,
			isLocked: await this.isLocked(),
			publicKey,
			derivationPath,
			lastUnlockedOn: await this.lastUnlockedOn,
			selected,
			nickname,
			isPasswordUnlockable: true,
			isKeyPairExportable: false,
		};
	}
}
