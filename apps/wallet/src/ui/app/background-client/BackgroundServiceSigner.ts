// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SerializedSignature, SignerWithProvider } from '@mysten/sui.js';

import type { BackgroundClient } from '.';
import type { Provider, SuiAddress } from '@mysten/sui.js';

export class BackgroundServiceSigner extends SignerWithProvider {
    readonly #address: SuiAddress;
    readonly #backgroundClient: BackgroundClient;

    constructor(
        address: SuiAddress,
        backgroundClient: BackgroundClient,
        provider?: Provider
    ) {
        super(provider);
        this.#address = address;
        this.#backgroundClient = backgroundClient;
    }

    async getAddress(): Promise<string> {
        return this.#address;
    }

    signData(data: Uint8Array): Promise<SerializedSignature> {
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
