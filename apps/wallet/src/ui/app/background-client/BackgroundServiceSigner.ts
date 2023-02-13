// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SignerWithProvider } from '@mysten/sui.js';

import type { BackgroundClient } from '.';
import type {
    Provider,
    SignaturePubkeyPair,
    SuiAddress,
    TxnDataSerializer,
} from '@mysten/sui.js';

export class BackgroundServiceSigner extends SignerWithProvider {
    readonly #address: SuiAddress;
    readonly #backgroundClient: BackgroundClient;

    constructor(
        address: SuiAddress,
        backgroundClient: BackgroundClient,
        provider?: Provider,
        serializer?: TxnDataSerializer
    ) {
        super(provider, serializer);
        this.#address = address;
        this.#backgroundClient = backgroundClient;
    }

    async getAddress(): Promise<string> {
        return this.#address;
    }

    signData(data: Uint8Array): Promise<SignaturePubkeyPair> {
        return this.#backgroundClient.signData(this.#address, data);
    }

    connect(provider: Provider): SignerWithProvider {
        return new BackgroundServiceSigner(
            this.#address,
            this.#backgroundClient,
            provider
        );
    }
}
