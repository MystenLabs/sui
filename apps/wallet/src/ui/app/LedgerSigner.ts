// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    Ed25519PublicKey,
    type SerializedSignature,
    type SignatureScheme,
    SignerWithProvider,
    type SuiAddress,
    toSerializedSignature,
    type JsonRpcProvider,
} from '@mysten/sui.js';

import type SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';

export class LedgerSigner extends SignerWithProvider {
    readonly #suiLedgerClient: SuiLedgerClient;
    readonly #derivationPath: string;
    readonly #signatureScheme: SignatureScheme = 'ED25519';

    constructor(
        suiLedgerClient: SuiLedgerClient,
        derivationPath: string,
        provider: JsonRpcProvider
    ) {
        super(provider);
        this.#suiLedgerClient = suiLedgerClient;
        this.#derivationPath = derivationPath;
    }

    async getAddress(): Promise<SuiAddress> {
        const publicKeyResult = await this.#suiLedgerClient.getPublicKey(
            this.#derivationPath
        );
        const publicKey = new Ed25519PublicKey(publicKeyResult.publicKey);
        return publicKey.toSuiAddress();
    }

    async getPublicKey(): Promise<Ed25519PublicKey> {
        const { publicKey } = await this.#suiLedgerClient.getPublicKey(
            this.#derivationPath
        );
        return new Ed25519PublicKey(publicKey);
    }

    async signData(data: Uint8Array): Promise<SerializedSignature> {
        const { signature } = await this.#suiLedgerClient.signTransaction(
            this.#derivationPath,
            data
        );
        const pubKey = await this.getPublicKey();
        return toSerializedSignature({
            signature,
            signatureScheme: this.#signatureScheme,
            pubKey,
        });
    }

    connect(provider: JsonRpcProvider): SignerWithProvider {
        return new LedgerSigner(
            this.#suiLedgerClient,
            this.#derivationPath,
            provider
        );
    }
}
