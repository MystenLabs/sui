// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { decrypt } from '_shared/cryptography/keystore';
import {
	entropyToMnemonic,
	mnemonicToEntropy,
	toEntropy,
	validateEntropy,
} from '_shared/utils/bip39';
import {
	fromExportedKeypair,
	type LegacyExportedKeyPair,
} from '_shared/utils/from-exported-keypair';
import { mnemonicToSeedHex, type Keypair } from '@mysten/sui/cryptography';

import { getFromLocalStorage } from '../storage-utils';

type StoredData = string | { v: 1 | 2; data: string };

type V2DecryptedDataType = {
	entropy: string;
	importedKeypairs: LegacyExportedKeyPair[];
	qredoTokens?: Record<string, string>;
	mnemonicSeedHex?: string;
};

const VAULT_KEY = 'vault';

export class LegacyVault {
	public readonly entropy: Uint8Array;
	public readonly importedKeypairs: Keypair[];
	public readonly qredoTokens: Map<string, string> = new Map();
	public readonly mnemonicSeedHex: string;

	public static async fromLegacyStorage(password: string) {
		const data = await getFromLocalStorage<StoredData>(VAULT_KEY);
		if (!data) {
			throw new Error('Wallet is not initialized');
		}
		let entropy: Uint8Array | null = null;
		let keypairs: Keypair[] = [];
		let qredoTokens = new Map<string, string>();
		let mnemonicSeedHex: string | null = null;
		if (typeof data === 'string') {
			entropy = mnemonicToEntropy(
				// eslint-disable-next-line no-restricted-globals
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
			keypairs = importedKeypairs.map((aKeyPair) => fromExportedKeypair(aKeyPair, true));
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
		return new LegacyVault(entropy, keypairs, qredoTokens, mnemonicSeedHex);
	}

	public static async isInitialized() {
		return !!(await getFromLocalStorage<StoredData>(VAULT_KEY));
	}

	public static async verifyPassword(password: string) {
		try {
			await LegacyVault.fromLegacyStorage(password);
			return true;
		} catch (e) {
			return false;
		}
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

	public getMnemonic() {
		return entropyToMnemonic(this.entropy);
	}
}
