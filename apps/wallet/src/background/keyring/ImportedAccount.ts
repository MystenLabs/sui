// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    normalizeSuiAddress,
    type SerializedSignature,
    type SoftwareKeypair,
    toSerializedSignature,
    type SuiAddress,
} from '@mysten/sui.js';

import { type Account, AccountType } from './Account';

export type SerializedImportedAccount = {
    type: AccountType.IMPORTED;
    address: SuiAddress;
    derivationPath: null;
};

export class ImportedAccount implements Account {
    #keypair: SoftwareKeypair;
    readonly type: AccountType;
    readonly address: SuiAddress;

    constructor({ keypair }: { keypair: SoftwareKeypair }) {
        this.type = AccountType.IMPORTED;
        this.#keypair = keypair;
        this.address = normalizeSuiAddress(
            this.#keypair.getPublicKey().toSuiAddress()
        );
    }

    sign(data: Uint8Array): SerializedSignature {
        const pubkey = this.#keypair.getPublicKey();
        // This is fine to hardcode useRecoverable = false because wallet does not support Secp256k1. Ed25519 does not use this parameter.
        const signature = this.#keypair.signData(data, false);
        const signatureScheme = this.#keypair.getKeyScheme();
        return toSerializedSignature({
            signature,
            signatureScheme,
            pubKey: pubkey,
        });
    }

    exportKeypair() {
        return this.#keypair.export();
    }

    toJSON(): SerializedImportedAccount {
        return {
            type: AccountType.IMPORTED,
            address: this.address,
            derivationPath: null,
        };
    }
}
