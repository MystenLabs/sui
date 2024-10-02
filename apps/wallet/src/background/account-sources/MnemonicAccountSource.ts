// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	entropyToMnemonic,
	entropyToSerialized,
	getRandomEntropy,
	toEntropy,
	validateEntropy,
} from '_shared/utils/bip39';
import { decrypt, encrypt } from '_src/shared/cryptography/keystore';
import { mnemonicToSeedHex } from '@mysten/sui/cryptography';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { sha256 } from '@noble/hashes/sha256';
import { bytesToHex } from '@noble/hashes/utils';
import Dexie from 'dexie';

import { getAccountSources } from '.';
import { getAllAccounts } from '../accounts';
import { MnemonicAccount, type MnemonicSerializedAccount } from '../accounts/MnemonicAccount';
import { setupAutoLockAlarm } from '../auto-lock-accounts';
import { backupDB, getDB } from '../db';
import { makeUniqueKey } from '../storage-utils';
import {
	AccountSource,
	type AccountSourceSerialized,
	type AccountSourceSerializedUI,
} from './AccountSource';
import { accountSourcesEvents } from './events';

type DataDecrypted = {
	entropyHex: string;
	mnemonicSeedHex: string;
};

interface MnemonicAccountSourceSerialized extends AccountSourceSerialized {
	type: 'mnemonic';
	encryptedData: string;
	// hash of entropy to be used for comparing sources (even when locked)
	sourceHash: string;
}

interface MnemonicAccountSourceSerializedUI extends AccountSourceSerializedUI {
	type: 'mnemonic';
}

export function makeDerivationPath(index: number) {
	// currently returns only Ed25519 path
	return `m/44'/784'/${index}'/0'/0'`;
}

export function deriveKeypairFromSeed(mnemonicSeedHex: string, derivationPath: string) {
	return Ed25519Keypair.deriveKeypairFromSeed(mnemonicSeedHex, derivationPath);
}

export class MnemonicAccountSource extends AccountSource<
	MnemonicAccountSourceSerialized,
	DataDecrypted
> {
	static async createNew({
		password,
		entropyInput,
	}: {
		password: string;
		entropyInput?: Uint8Array;
	}) {
		const entropy = entropyInput || getRandomEntropy();
		if (!validateEntropy(entropy)) {
			throw new Error("Can't create Mnemonic account source, invalid entropy");
		}
		const dataSerialized: MnemonicAccountSourceSerialized = {
			id: makeUniqueKey(),
			type: 'mnemonic',
			encryptedData: await MnemonicAccountSource.createEncryptedData(entropy, password),
			sourceHash: bytesToHex(sha256(entropy)),
			createdAt: Date.now(),
		};
		const allAccountSources = await getAccountSources();
		for (const anAccountSource of allAccountSources) {
			if (
				anAccountSource instanceof MnemonicAccountSource &&
				(await anAccountSource.sourceHash) === dataSerialized.sourceHash
			) {
				throw new Error('Mnemonic account source already exists');
			}
		}
		return dataSerialized;
	}

	static isOfType(
		serialized: AccountSourceSerialized,
	): serialized is MnemonicAccountSourceSerialized {
		return serialized.type === 'mnemonic';
	}

	static async save(
		serialized: MnemonicAccountSourceSerialized,
		{
			skipBackup = false,
			skipEventEmit = false,
		}: { skipBackup?: boolean; skipEventEmit?: boolean } = {},
	) {
		await (await Dexie.waitFor(getDB())).accountSources.put(serialized);
		if (!skipBackup) {
			await backupDB();
		}
		if (!skipEventEmit) {
			accountSourcesEvents.emit('accountSourcesChanged');
		}
		return new MnemonicAccountSource(serialized.id);
	}

	static createEncryptedData(entropy: Uint8Array, password: string) {
		const decryptedData: DataDecrypted = {
			entropyHex: entropyToSerialized(entropy),
			mnemonicSeedHex: mnemonicToSeedHex(entropyToMnemonic(entropy)),
		};
		return encrypt(password, decryptedData);
	}

	constructor(id: string) {
		super({ type: 'mnemonic', id });
	}

	async isLocked() {
		return (await this.getEphemeralValue()) === null;
	}

	async unlock(password: string) {
		await this.setEphemeralValue(await this.#decryptStoredData(password));
		await setupAutoLockAlarm();
		accountSourcesEvents.emit('accountSourceStatusUpdated', { accountSourceID: this.id });
	}

	async verifyPassword(password: string) {
		const { encryptedData } = await this.getStoredData();
		await decrypt<DataDecrypted>(password, encryptedData);
	}

	async lock() {
		await this.clearEphemeralValue();
		accountSourcesEvents.emit('accountSourceStatusUpdated', { accountSourceID: this.id });
	}

	async deriveAccount({ derivationPathIndex }: { derivationPathIndex?: number } = {}): Promise<
		Omit<MnemonicSerializedAccount, 'id'>
	> {
		const derivationPath =
			typeof derivationPathIndex !== 'undefined'
				? makeDerivationPath(derivationPathIndex)
				: await this.#getAvailableDerivationPath();
		const keyPair = await this.deriveKeyPair(derivationPath);
		return MnemonicAccount.createNew({ keyPair, derivationPath, sourceID: this.id });
	}

	async deriveKeyPair(derivationPath: string) {
		const data = await this.getEphemeralValue();
		if (!data) {
			throw new Error(`Mnemonic account source ${this.id} is locked`);
		}
		return deriveKeypairFromSeed(data.mnemonicSeedHex, derivationPath);
	}

	async toUISerialized(): Promise<MnemonicAccountSourceSerializedUI> {
		const { type } = await this.getStoredData();
		return {
			id: this.id,
			type,
			isLocked: await this.isLocked(),
		};
	}

	async getEntropy(password?: string) {
		let data = await this.getEphemeralValue();
		if (password && !data) {
			data = await this.#decryptStoredData(password);
		}
		if (!data) {
			throw new Error(`Mnemonic account source ${this.id} is locked`);
		}
		return data.entropyHex;
	}

	get sourceHash() {
		return this.getStoredData().then(({ sourceHash }) => sourceHash);
	}

	async verifyRecoveryData(entropy: string) {
		const newEntropyHash = bytesToHex(sha256(toEntropy(entropy)));
		if (newEntropyHash !== (await this.sourceHash)) {
			throw new Error("Wrong passphrase, doesn't match the existing one");
		}
		return true;
	}

	async #getAvailableDerivationPath() {
		const derivationPathMap: Record<string, boolean> = {};
		for (const anAccount of await getAllAccounts({ sourceID: this.id })) {
			if (anAccount instanceof MnemonicAccount && (await anAccount.sourceID) === this.id) {
				derivationPathMap[await anAccount.derivationPath] = true;
			}
		}
		let index = 0;
		let derivationPath = '';
		let temp;
		do {
			temp = makeDerivationPath(index++);
			if (!derivationPathMap[temp]) {
				derivationPath = temp;
			}
		} while (derivationPath === '' && index < 10000);
		if (!derivationPath) {
			throw new Error('Failed to find next available derivation path');
		}
		return derivationPath;
	}

	async #decryptStoredData(password: string) {
		const { encryptedData } = await this.getStoredData();
		return decrypt<DataDecrypted>(password, encryptedData);
	}
}
