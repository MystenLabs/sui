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
    readonly #forceLedgerConnection: () => Promise<SuiLedgerClient>;
    readonly #derivationPath: string;
    readonly #signatureScheme: SignatureScheme = 'ED25519';

    constructor(
        forceLedgerConnection: () => Promise<SuiLedgerClient>,
        derivationPath: string,
        provider: JsonRpcProvider
    ) {
        super(provider);
        this.#forceLedgerConnection = forceLedgerConnection;
        this.#derivationPath = derivationPath;
    }

    async getAddress(): Promise<SuiAddress> {
        const ledgerClient = await this.#forceLedgerConnection();
        const publicKeyResult = await ledgerClient.getPublicKey(
            this.#derivationPath
        );
        const publicKey = new Ed25519PublicKey(publicKeyResult.publicKey);
        return publicKey.toSuiAddress();
    }

    async getPublicKey(): Promise<Ed25519PublicKey> {
        const ledgerClient = await this.#forceLedgerConnection();
        const { publicKey } = await ledgerClient.getPublicKey(
            this.#derivationPath
        );
        return new Ed25519PublicKey(publicKey);
    }

    async signData(data: Uint8Array): Promise<SerializedSignature> {
        const ledgerClient = await this.#forceLedgerConnection();
        const { signature } = await ledgerClient.signTransaction(
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
            this.#forceLedgerConnection,
            this.#derivationPath,
            provider
        );
    }
}
