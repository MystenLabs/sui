// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair, mnemonicToSeedHex } from '@mysten/sui.js';
import { sha256 } from '@noble/hashes/sha256';

import { bytesToHex } from '@noble/hashes/utils';
import { getAccountSources } from '.';
import {
	AccountSource,
	type AccountSourceSerializedUI,
	type AccountSourceSerialized,
} from './AccountSource';
import { addNewAccount } from '../accounts';
import { type MnemonicSerializedAccount } from '../accounts/MnemonicAccount';
import { setStorageEntity } from '../storage-entities-utils';
import { makeUniqueKey } from '../storage-utils';
import { getRandomEntropy, entropyToSerialized, entropyToMnemonic } from '_shared/utils/bip39';
import { decrypt, encrypt } from '_src/shared/cryptography/keystore';

export class MnemonicAccountSource extends AccountSource<
	MnemonicAccountSourceSerialized,
	DataDecryptedV0
> {
	static async createNew({
		password,
		entropyInput,
	}: {
		password: string;
		entropyInput?: Uint8Array;
	}) {
		const entropy = entropyInput || getRandomEntropy();
		const decryptedData: DataDecryptedV0 = {
			version: 0,
			entropyHex: entropyToSerialized(entropy),
			mnemonicSeedHex: mnemonicToSeedHex(entropyToMnemonic(entropy)),
		};
		const dataSerialized: MnemonicAccountSourceSerialized = {
			id: makeUniqueKey(),
			storageEntityType: 'account-source-entity',
			type: 'mnemonic',
			encrypted: await encrypt(password, decryptedData),
			lastAccountIndex: null,
			sourceHash: bytesToHex(sha256(entropy)),
		};
		const allAccountSources = await getAccountSources({ type: 'mnemonic' });
		for (const anAccountSource of allAccountSources) {
			if ((await anAccountSource.sourceHash) === dataSerialized.sourceHash) {
				throw new Error('Mnemonic account source already exists');
			}
		}
		await setStorageEntity(dataSerialized);
		return new MnemonicAccountSource(dataSerialized.id);
	}

	static isOfType(
		serialized: AccountSourceSerialized,
	): serialized is MnemonicAccountSourceSerialized {
		return serialized.type === 'mnemonic';
	}

	constructor(id: string) {
		super({ type: 'mnemonic', id });
	}

	async isLocked() {
		return (await this.getEphemeralValue()) === null;
	}

	async unlock(password: string) {
		return this.setEphemeralValue(await decrypt(password, (await this.getStoredData()).encrypted));
	}

	lock() {
		return this.clearEphemeralValue();
	}

	async deriveAccount() {
		const currentIndex = (await this.getStoredData()).lastAccountIndex;
		const nextIndex = currentIndex === null ? 0 : currentIndex + 1;
		const derivationPath = this.makeDerivationPath(nextIndex);
		const keyPair = await this.deriveKeyPair(derivationPath);
		const newAccount = await addNewAccount<MnemonicSerializedAccount>({
			type: 'mnemonic-derived',
			storageEntityType: 'account-entity',
			sourceID: this.id,
			address: keyPair.getPublicKey().toSuiAddress(),
			derivationPath,
			publicKey: keyPair.getPublicKey().toBase64(),
		});
		await this.updateStoredData({ lastAccountIndex: nextIndex });
		return newAccount;
	}

	async deriveKeyPair(derivationPath: string) {
		const data = await this.getEphemeralValue();
		if (!data) {
			throw new Error('Locked');
		}
		return Ed25519Keypair.deriveKeypairFromSeed(data.mnemonicSeedHex, derivationPath);
	}

	async toUISerialized(): Promise<MnemonicAccountSourceSerializedUI> {
		const { type } = await this.getStoredData();
		return {
			id: this.id,
			type,
			isLocked: await this.isLocked(),
		};
	}

	get lastAccountIndex() {
		return this.getStoredData().then(({ lastAccountIndex }) => lastAccountIndex);
	}

	get sourceHash() {
		return this.getStoredData().then(({ sourceHash }) => sourceHash);
	}

	private makeDerivationPath(index: number) {
		// currently returns only Ed25519 path
		return `m/44'/784'/${index}'/0'/0'`;
	}
}

type DataDecryptedV0 = {
	version: 0;
	entropyHex: string;
	mnemonicSeedHex: string;
};

interface MnemonicAccountSourceSerialized extends AccountSourceSerialized {
	type: 'mnemonic';
	encrypted: string;
	lastAccountIndex: number | null;
	// hash of entropy to be used for comparing sources (even when locked)
	sourceHash: string;
}

interface MnemonicAccountSourceSerializedUI extends AccountSourceSerializedUI {}
