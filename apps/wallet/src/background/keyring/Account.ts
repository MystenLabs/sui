// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    normalizeSuiAddress,
    SIGNATURE_SCHEME_TO_FLAG,
    toB64,
} from '@mysten/sui.js';

import type { SignaturePubkeyPair, Keypair, SuiAddress } from '@mysten/sui.js';

export type AccountType = 'derived' | 'imported';
export type AccountSerialized = {
    type: AccountType;
    address: SuiAddress;
    derivationPath: string | null;
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

    // TODO: Ideally we can make `KeyPair` own the full `SignaturePubkeyPair` signing structure.
    async sign(data: Uint8Array): Promise<SignaturePubkeyPair> {
        const pubkey = this.#keypair.getPublicKey();
        // This is fine to hardcode useRecoverable = false because wallet does not support Secp256k1. Ed25519 does not use this parameter.
        const signature = this.#keypair.signData(data, false);
        const signatureScheme = this.#keypair.getKeyScheme();

        const serialized_sig = new Uint8Array(
            1 + signature.length + pubkey.toBytes().length
        );
        serialized_sig.set([SIGNATURE_SCHEME_TO_FLAG[signatureScheme]]);
        serialized_sig.set(signature, 1);
        serialized_sig.set(pubkey.toBytes(), 1 + signature.length);

        return {
            signatureScheme,
            signature: toB64(signature),
            pubKey: pubkey.toBase64(),
            serializedSignature: toB64(serialized_sig),
        };
    }

    toJSON(): AccountSerialized {
        return {
            type: this.type,
            address: this.address,
            derivationPath: this.derivationPath,
        };
    }
}
