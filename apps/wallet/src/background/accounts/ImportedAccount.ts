// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { decrypt, encrypt } from '_src/shared/cryptography/keystore';
import {
	fromExportedKeypair,
	type LegacyExportedKeyPair,
} from '_src/shared/utils/from-exported-keypair';

import {
	Account,
	type KeyPairExportableAccount,
	type PasswordUnlockableAccount,
	type SerializedAccount,
	type SerializedUIAccount,
	type SigningAccount,
} from './Account';

type SessionStorageData = { keyPair: LegacyExportedKeyPair | string };
type EncryptedData = { keyPair: LegacyExportedKeyPair | string };

export interface ImportedAccountSerialized extends SerializedAccount {
	type: 'imported';
	encrypted: string;
	publicKey: string;
}

export interface ImportedAccountSerializedUI extends SerializedUIAccount {
	type: 'imported';
	publicKey: string;
}

export function isImportedAccountSerializedUI(
	account: SerializedUIAccount,
): account is ImportedAccountSerializedUI {
	return account.type === 'imported';
}

export class ImportedAccount
	extends Account<ImportedAccountSerialized, SessionStorageData>
	implements PasswordUnlockableAccount, SigningAccount, KeyPairExportableAccount
{
	readonly canSign = true;
	readonly unlockType = 'password' as const;
	readonly exportableKeyPair = true;

	static async createNew(inputs: {
		keyPair: string;
		password: string;
	}): Promise<Omit<ImportedAccountSerialized, 'id'>> {
		const keyPair = fromExportedKeypair(inputs.keyPair);
		const dataToEncrypt: EncryptedData = {
			keyPair: inputs.keyPair,
		};
		return {
			type: 'imported',
			address: keyPair.getPublicKey().toSuiAddress(),
			publicKey: keyPair.getPublicKey().toBase64(),
			encrypted: await encrypt(inputs.password, dataToEncrypt),
			lastUnlockedOn: null,
			selected: false,
			nickname: null,
			createdAt: Date.now(),
		};
	}

	static isOfType(serialized: SerializedAccount): serialized is ImportedAccountSerialized {
		return serialized.type === 'imported';
	}

	constructor({ id, cachedData }: { id: string; cachedData?: ImportedAccountSerialized }) {
		super({ type: 'imported', id, cachedData });
	}

	async lock(allowRead = false): Promise<void> {
		await this.clearEphemeralValue();
		await this.onLocked(allowRead);
	}

	async isLocked(): Promise<boolean> {
		return !(await this.#getKeyPair());
	}

	async toUISerialized(): Promise<ImportedAccountSerializedUI> {
		const { address, publicKey, type, selected, nickname } = await this.getStoredData();
		return {
			id: this.id,
			type,
			address,
			publicKey,
			isLocked: await this.isLocked(),
			lastUnlockedOn: await this.lastUnlockedOn,
			selected,
			nickname,
			isPasswordUnlockable: true,
			isKeyPairExportable: true,
		};
	}

	async passwordUnlock(password?: string): Promise<void> {
		if (!password) {
			throw new Error('Missing password to unlock the account');
		}
		const { encrypted } = await this.getStoredData();
		const { keyPair } = await decrypt<EncryptedData>(password, encrypted);
		await this.setEphemeralValue({ keyPair });
		await this.onUnlocked();
	}

	async verifyPassword(password: string): Promise<void> {
		const { encrypted } = await this.getStoredData();
		await decrypt<EncryptedData>(password, encrypted);
	}

	async signData(data: Uint8Array): Promise<string> {
		const keyPair = await this.#getKeyPair();
		if (!keyPair) {
			throw new Error(`Account is locked`);
		}
		return this.generateSignature(data, keyPair);
	}

	async exportKeyPair(password: string): Promise<string> {
		const { encrypted } = await this.getStoredData();
		const { keyPair } = await decrypt<EncryptedData>(password, encrypted);
		return fromExportedKeypair(keyPair, true).getSecretKey();
	}

	async #getKeyPair() {
		const ephemeralData = await this.getEphemeralValue();
		if (ephemeralData) {
			return fromExportedKeypair(ephemeralData.keyPair, true);
		}
		return null;
	}
}
