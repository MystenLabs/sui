// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { encrypt, decrypt } from '_shared/cryptography/keystore';
import {
    entropyToMnemonic,
    entropyToSerialized,
    mnemonicToEntropy,
    toEntropy,
    validateEntropy,
} from '_shared/utils/bip39';

export const LATEST_VAULT_VERSION = 1;

export type StoredData = string | { v: number; data: string };

/**
 * Holds the mnemonic of the wallet and provides functionality to create/encrypt/decrypt it.
 */
export class Vault {
    public readonly entropy: Uint8Array;

    public static async from(
        password: string,
        data: StoredData,
        onMigrateCallback?: (vault: Vault) => Promise<void>
    ) {
        let entropy: Uint8Array | null = null;
        let doMigrate = false;
        if (typeof data === 'string') {
            entropy = mnemonicToEntropy(
                Buffer.from(await decrypt<string>(password, data)).toString(
                    'utf-8'
                )
            );
            doMigrate = true;
        } else if (data.v === 1) {
            entropy = toEntropy(await decrypt<string>(password, data.data));
        } else {
            throw new Error(
                "Unknown data, provided data can't be used to create a Vault"
            );
        }
        if (!validateEntropy(entropy)) {
            throw new Error("Can't restore Vault, entropy is invalid.");
        }
        const vault = new Vault(entropy);
        if (doMigrate && typeof onMigrateCallback === 'function') {
            await onMigrateCallback(vault);
        }
        return vault;
    }

    constructor(entropy: Uint8Array) {
        this.entropy = entropy;
    }

    public async encrypt(password: string) {
        return {
            v: LATEST_VAULT_VERSION,
            data: await encrypt(password, entropyToSerialized(this.entropy)),
        };
    }

    public getMnemonic() {
        return entropyToMnemonic(this.entropy);
    }
}
