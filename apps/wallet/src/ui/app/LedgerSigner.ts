// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    SignerWithProvider,
    Ed25519PublicKey,
    Base64DataBuffer,
} from '@mysten/sui.js';

import WebHIDTransport from '@ledgerhq/hw-transport-webhid';
import WebUSBTransport from '@ledgerhq/hw-transport-webusb';
import type Transport from '@ledgerhq/hw-transport';
import type AppSui from 'hw-app-sui';
import type {
    Provider,
    SignaturePubkeyPair,
    SuiAddress,
    TxnDataSerializer,
} from '@mysten/sui.js';

export class LedgerSigner extends SignerWithProvider {
    readonly #appSui: Promise<AppSui>;
    readonly #derivationPath: string;
    constructor(
        appSui: Promise<AppSui>,
        derivationPath: string,
        provider?: Provider,
        serializer?: TxnDataSerializer
    ) {
        super(provider, serializer);
        this.#appSui = appSui;
        this.#derivationPath = derivationPath;
    }

    async getAddress(): Promise<string> {
        return (
            '0x' +
            new Buffer(
                (
                    await (
                        await this.#appSui
                    ).getPublicKey(this.#derivationPath)
                ).address
            ).toString('hex')
        );
    }

    async getPublicKey(): Promise<Ed25519PublicKey> {
        const { publicKey } = await (
            await this.#appSui
        ).getPublicKey(this.#derivationPath);
        return new Ed25519PublicKey(publicKey);
    }

    async signData(data: Base64DataBuffer): Promise<SignaturePubkeyPair> {
        const { signature } = await (
            await this.#appSui
        ).signTransaction(this.#derivationPath, data.getData());
        return {
            signatureScheme: 'ED25519',
            signature: new Base64DataBuffer(signature),
            pubKey: await this.getPublicKey(),
        };
    }

    connect(provider: Provider): SignerWithProvider {
        return new LedgerSigner(this.#appSui, this.#derivationPath, provider);
    }
}
