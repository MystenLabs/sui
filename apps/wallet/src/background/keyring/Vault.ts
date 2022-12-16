// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromExportedKeypair } from '@mysten/sui.js';

import { encrypt, decrypt } from '_shared/cryptography/keystore';
import {
    entropyToMnemonic,
    entropyToSerialized,
    mnemonicToEntropy,
    toEntropy,
    validateEntropy,
} from '_shared/utils/bip39';

import type { ExportedKeypair, Keypair } from '@mysten/sui.js';

export const LATEST_VAULT_VERSION = 2;

export type StoredData = string | { v: 1 | 2; data: string };

export type V2DecryptedDataType = {
    entropy: string;
    importedKeypairs: ExportedKeypair[];
};

/**
 * Holds the mnemonic of the wallet and any imported Keypairs.
 * Also provides functionality to create/encrypt/decrypt it.
 */
export class Vault {
    public static async from(
        password: string,
        data: StoredData,
        onMigrateCallback?: (vault: Vault) => Promise<void>
    ) {
        let entropy: Uint8Array | null = null;
        let keypairs: Keypair[] = [];
        if (typeof data === 'string') {
            entropy = mnemonicToEntropy(
                Buffer.from(await decrypt<string>(password, data)).toString(
                    'utf-8'
                )
            );
        } else if (data.v === 1) {
            entropy = toEntropy(await decrypt<string>(password, data.data));
        } else if (data.v === 2) {
            const { entropy: entropySerialized, importedKeypairs } =
                await decrypt<V2DecryptedDataType>(password, data.data);
            entropy = toEntropy(entropySerialized);
            keypairs = importedKeypairs.map(fromExportedKeypair);
        } else {
            throw new Error(
                "Unknown data, provided data can't be used to create a Vault"
            );
        }
        if (!validateEntropy(entropy)) {
            throw new Error("Can't restore Vault, entropy is invalid.");
        }
        const vault = new Vault(entropy, keypairs);
        const doMigrate =
            typeof data === 'string' || data.v !== LATEST_VAULT_VERSION;
        if (doMigrate && typeof onMigrateCallback === 'function') {
            await onMigrateCallback(vault);
        }
        return vault;
    }

    constructor(
        public readonly entropy: Uint8Array,
        public readonly importedKeypairs: Keypair[] = []
    ) {}

    public async encrypt(password: string) {
        const dataToEncrypt: V2DecryptedDataType = {
            entropy: entropyToSerialized(this.entropy),
            importedKeypairs: this.importedKeypairs.map((aKeypair) =>
                aKeypair.export()
            ),
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
