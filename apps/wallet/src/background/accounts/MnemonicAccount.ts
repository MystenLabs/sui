// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ExportedKeypair, fromExportedKeypair } from '@mysten/sui.js';
import {
	Account,
	type SerializedAccount,
	type PasswordUnLockableAccount,
	type SerializedUIAccount,
	type SigningAccount,
} from './Account';
import { MnemonicAccountSource } from '../account-sources/MnemonicAccountSource';

export interface MnemonicSerializedAccount extends Omit<SerializedAccount, 'publicKey'> {
	type: 'mnemonic-derived';
	sourceID: string;
	derivationPath: string;
	publicKey: string;
}

export interface MnemonicSerializedUiAccount extends Omit<SerializedUIAccount, 'publicKey'> {
	derivationPath: string;
	publicKey: string;
	sourceID: string;
}

type SessionStorageData = { keyPair: ExportedKeypair };

export class MnemonicAccount
	extends Account<MnemonicSerializedAccount, SessionStorageData>
	implements PasswordUnLockableAccount, SigningAccount
{
	readonly unlockType = 'password' as const;
	readonly canSign = true;

	static isOfType(serialized: SerializedAccount): serialized is MnemonicSerializedAccount {
		return serialized.type === 'mnemonic-derived';
	}

	constructor({ id }: { id: string }) {
		super({ type: 'mnemonic-derived', id });
	}

	async isLocked(): Promise<boolean> {
		return !(await this.#getKeyPair());
	}

	lock(): Promise<void> {
		return this.clearEphemeralValue();
	}

	async passwordUnlock(password: string): Promise<void> {
		const { derivationPath } = await this.getStoredData();
		const mnemonicSource = await this.#getMnemonicSource();
		await mnemonicSource.unlock(password);
		return this.setEphemeralValue({
			keyPair: (await mnemonicSource.deriveKeyPair(derivationPath)).export(),
		});
	}

	async toUISerialized(): Promise<MnemonicSerializedUiAccount> {
		const { id, type, address, derivationPath, publicKey, sourceID } = await this.getStoredData();
		return {
			id,
			type,
			address,
			isLocked: await this.isLocked(),
			derivationPath,
			publicKey,
			sourceID,
		};
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

	async #getMnemonicSource() {
		return new MnemonicAccountSource((await this.getStoredData()).sourceID);
	}
}
