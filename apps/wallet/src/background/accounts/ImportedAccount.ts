// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromExportedKeypair, type ExportedKeypair } from '@mysten/sui.js';
import {
	Account,
	type PasswordUnLockableAccount,
	type SerializedUIAccount,
	type SigningAccount,
	type SerializedAccount,
} from './Account';
import { decrypt, encrypt } from '_src/shared/cryptography/keystore';

type SessionStorageData = { keyPair: ExportedKeypair };
type EncryptedDataV0 = { keyPair: ExportedKeypair };
type EncryptedData = EncryptedDataV0;

export interface ImportedAccountSerialized extends SerializedAccount {
	type: 'imported';
	encrypted: string;
	publicKey: string;
}

export interface ImportedAccountSerializedUI extends SerializedUIAccount {
	type: 'imported';
	publicKey: string;
}

export class ImportedAccount
	extends Account<ImportedAccountSerialized, SessionStorageData>
	implements PasswordUnLockableAccount, SigningAccount
{
	readonly canSign = true;
	readonly unlockType = 'password' as const;

	static async createNew(inputs: {
		keyPair: ExportedKeypair;
		password: string;
	}): Promise<Omit<ImportedAccountSerialized, 'id'>> {
		const keyPair = fromExportedKeypair(inputs.keyPair);
		const dataToEncrypt: EncryptedDataV0 = {
			keyPair: inputs.keyPair,
		};
		return {
			storageEntityType: 'account-entity',
			type: 'imported',
			address: keyPair.getPublicKey().toSuiAddress(),
			publicKey: keyPair.getPublicKey().toBase64(),
			encrypted: await encrypt(inputs.password, dataToEncrypt),
		};
	}

	static isOfType(serialized: SerializedAccount): serialized is ImportedAccountSerialized {
		return serialized.type === 'imported';
	}

	constructor({ id }: { id: string }) {
		super({ type: 'imported', id });
	}

	lock(): Promise<void> {
		return this.clearEphemeralValue();
	}

	async isLocked(): Promise<boolean> {
		return !(await this.#getKeyPair());
	}

	async toUISerialized(): Promise<SerializedUIAccount> {
		const { address, publicKey } = await this.getStoredData();
		return {
			id: this.id,
			type: this.type,
			address,
			publicKey,
			isLocked: await this.isLocked(),
		};
	}

	async passwordUnlock(password: string): Promise<void> {
		const { encrypted } = await this.getStoredData();
		const { keyPair } = await decrypt<EncryptedData>(password, encrypted);
		return this.setEphemeralValue({ keyPair });
	}

	async signData(data: Uint8Array): Promise<string> {
		const keyPair = await this.#getKeyPair();
		if (!keyPair) {
			throw new Error(`Account is locked`);
		}
		return this.generateSignature(data, keyPair);
	}

	async #getKeyPair() {
		const ephemeralData = await this.getEphemeralValue();
		if (ephemeralData) {
			return fromExportedKeypair(ephemeralData.keyPair);
		}
		return null;
	}
}
