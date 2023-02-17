// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    normalizeSuiAddress,
    toSerializedSignature,
    type SerializedSignature,
    type Keypair,
    type SuiAddress,
} from '@mysten/sui.js';

export type AccountType = 'derived' | 'imported';
export type AccountSerialized =
    | {
          type: 'derived';
          address: SuiAddress;
          derivationPath: string;
      }
    | {
          type: 'imported';
          address: SuiAddress;
          derivationPath: null;
      };

export class Account {
    #keypair: Keypair;
    public readonly type: AccountType;
    public readonly derivationPath: string | null;
    public readonly address: SuiAddress;

    constructor(
        options:
            | { type: 'derived'; derivationPath: string; keypair: Keypair }
            | { type: 'imported'; keypair: Keypair }
    ) {
        this.type = options.type;
        this.derivationPath =
            options.type === 'derived' ? options.derivationPath : null;
        this.#keypair = options.keypair;
        this.address = normalizeSuiAddress(
            this.#keypair.getPublicKey().toSuiAddress()
        );
    }

    exportKeypair() {
        return this.#keypair.export();
    }

    async sign(data: Uint8Array): Promise<SerializedSignature> {
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

    toJSON(): AccountSerialized {
        switch (this.type) {
            case 'derived':
                if (this.derivationPath === null) {
                    throw new Error(
                        'Error, derived path account missing derived path'
                    );
                }
                return {
                    type: 'derived',
                    address: this.address,
                    derivationPath: this.derivationPath,
                };
            case 'imported':
                if (this.derivationPath !== null) {
                    throw new Error(
                        'Error, imported path account has derived path'
                    );
                }
                return {
                    type: 'imported',
                    address: this.address,
                    derivationPath: this.derivationPath,
                };
            default:
                throw new Error('Error, unknown account type');
        }
    }
}
