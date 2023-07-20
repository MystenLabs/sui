// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromExportedKeypair } from '@mysten/sui.js';
import { mnemonicToSeedHex, type ExportedKeypair, type Keypair } from '@mysten/sui.js/cryptography';

import { encrypt, decrypt } from '_shared/cryptography/keystore';
import {
	entropyToMnemonic,
	entropyToSerialized,
	mnemonicToEntropy,
	toEntropy,
	validateEntropy,
} from '_shared/utils/bip39';

export const LATEST_VAULT_VERSION = 2;

export type StoredData = string | { v: 1 | 2; data: string };

export type V2DecryptedDataType = {
	entropy: string;
	importedKeypairs: ExportedKeypair[];
	qredoTokens?: Record<string, string>;
	mnemonicSeedHex?: string;
};

/**
 * Holds the mnemonic of the wallet and any imported Keypairs.
 * Also provides functionality to create/encrypt/decrypt it.
 */
export class Vault {
	public readonly entropy: Uint8Array;
	public readonly importedKeypairs: Keypair[];
	public readonly qredoTokens: Map<string, string> = new Map();
	public readonly mnemonicSeedHex: string;

	public static async from(
		password: string,
		data: StoredData,
		onMigrateCallback?: (vault: Vault) => Promise<void>,
	) {
		let entropy: Uint8Array | null = null;
		let keypairs: Keypair[] = [];
		let qredoTokens = new Map<string, string>();
		let mnemonicSeedHex: string | null = null;
		if (typeof data === 'string') {
			entropy = mnemonicToEntropy(
				Buffer.from(await decrypt<string>(password, data)).toString('utf-8'),
			);
		} else if (data.v === 1) {
			entropy = toEntropy(await decrypt<string>(password, data.data));
		} else if (data.v === 2) {
			const {
				entropy: entropySerialized,
				importedKeypairs,
				qredoTokens: storedTokens,
				mnemonicSeedHex: storedMnemonicSeedHex,
			} = await decrypt<V2DecryptedDataType>(password, data.data);
			entropy = toEntropy(entropySerialized);
			keypairs = importedKeypairs.map(fromExportedKeypair);
			if (storedTokens) {
				qredoTokens = new Map(Object.entries(storedTokens));
			}
			mnemonicSeedHex = storedMnemonicSeedHex || null;
		} else {
			throw new Error("Unknown data, provided data can't be used to create a Vault");
		}
		if (!validateEntropy(entropy)) {
			throw new Error("Can't restore Vault, entropy is invalid.");
		}
		const vault = new Vault(entropy, keypairs, qredoTokens, mnemonicSeedHex);
		const doMigrate =
			typeof data === 'string' || data.v !== LATEST_VAULT_VERSION || !mnemonicSeedHex;
		if (doMigrate && typeof onMigrateCallback === 'function') {
			await onMigrateCallback(vault);
		}
		return vault;
	}

	constructor(
		entropy: Uint8Array,
		importedKeypairs: Keypair[] = [],
		qredoTokens: Map<string, string> = new Map(),
		mnemonicSeedHex: string | null = null,
	) {
		this.entropy = entropy;
		this.importedKeypairs = importedKeypairs;
		this.qredoTokens = qredoTokens;
		this.mnemonicSeedHex = mnemonicSeedHex || mnemonicToSeedHex(entropyToMnemonic(entropy));
	}

	public async encrypt(password: string) {
		const dataToEncrypt: V2DecryptedDataType = {
			entropy: entropyToSerialized(this.entropy),
			importedKeypairs: this.importedKeypairs.map((aKeypair) => aKeypair.export()),
			qredoTokens: Object.fromEntries(this.qredoTokens.entries()),
			mnemonicSeedHex: this.mnemonicSeedHex,
		};
		return {
			v: LATEST_VAULT_VERSION,
			data: await encrypt(password, dataToEncrypt),
		};
	}

	public getMnemonic() {
		return entropyToMnemonic(this.entropy);
	}
}
